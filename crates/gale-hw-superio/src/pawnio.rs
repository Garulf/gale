use std::ffi::{c_char, OsString};
use std::os::windows::ffi::OsStrExt;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

use crate::ffi_util::{duration_to_wait_ms, hresult_failed, hresult_message, wait_outcome, IOCTLS};
use crate::module_blob::{LPCIO_BIN, PAWNIO_MODULES_VERSION};
use crate::transport::PortIo;
use crate::SuperIoStatus;

pub const LIBRARY_NAME: &str = "PawnIOLib.dll";
pub const LIBRARY_ENV_OVERRIDE: &str = "GALE_PAWNIOLIB";
pub const ISA_MUTEX_NAME: &str = "Global\\Access_ISABUS.HTP.Method";
pub const UNINSTALL_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PawnIO";

const _: () = assert!(IOCTLS.len() == 7);

type PawnioVersion = unsafe extern "system" fn(*mut u32) -> i32;
type PawnioOpen = unsafe extern "system" fn(*mut HANDLE) -> i32;
type PawnioLoad = unsafe extern "system" fn(HANDLE, *const u8, usize) -> i32;
type PawnioExecute = unsafe extern "system" fn(
    HANDLE,
    *const c_char,
    *const u64,
    usize,
    *mut u64,
    usize,
    *mut usize,
) -> i32;
type PawnioClose = unsafe extern "system" fn(HANDLE) -> i32;

pub struct PawnIoTransport {
    _library: libloading::Library,
    execute: PawnioExecute,
    close: PawnioClose,
    handle: HANDLE,
    mutex: HANDLE,
    version: u32,
}

unsafe impl Send for PawnIoTransport {}

fn wide(value: &str) -> Vec<u16> {
    OsString::from(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn is_installed() -> bool {
    let key_path = wide(UNINSTALL_KEY);
    let mut hkey: HKEY = std::ptr::null_mut();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            key_path.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        )
    };
    if result != 0 {
        return false;
    }
    unsafe {
        RegCloseKey(hkey);
    }
    true
}

fn candidate_libraries() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var(LIBRARY_ENV_OVERRIDE) {
        candidates.push(path);
    }
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(format!("{program_files}\\PawnIO\\PawnIOLib.dll"));
    }
    candidates.push(LIBRARY_NAME.to_string());
    candidates
}

fn load_library() -> Result<libloading::Library, String> {
    let mut last_error = "PawnIOLib.dll could not be found".to_string();
    for path in candidate_libraries() {
        match unsafe { libloading::Library::new(path.as_str()) } {
            Ok(library) => return Ok(library),
            Err(err) => last_error = err.to_string(),
        }
    }
    Err(last_error)
}

pub fn open() -> Result<PawnIoTransport, SuperIoStatus> {
    if !is_installed() {
        return Err(SuperIoStatus::PawnIoMissing);
    }

    let library = load_library().map_err(SuperIoStatus::LibraryLoadFailed)?;

    let version_fn = *unsafe { library.get::<PawnioVersion>(b"pawnio_version\0") }
        .map_err(|err| SuperIoStatus::LibraryLoadFailed(err.to_string()))?;
    let open_fn = *unsafe { library.get::<PawnioOpen>(b"pawnio_open\0") }
        .map_err(|err| SuperIoStatus::LibraryLoadFailed(err.to_string()))?;
    let load_fn = *unsafe { library.get::<PawnioLoad>(b"pawnio_load\0") }
        .map_err(|err| SuperIoStatus::LibraryLoadFailed(err.to_string()))?;
    let execute_fn = *unsafe { library.get::<PawnioExecute>(b"pawnio_execute\0") }
        .map_err(|err| SuperIoStatus::LibraryLoadFailed(err.to_string()))?;
    let close_fn = *unsafe { library.get::<PawnioClose>(b"pawnio_close\0") }
        .map_err(|err| SuperIoStatus::LibraryLoadFailed(err.to_string()))?;

    let mut version = 0u32;
    if hresult_failed(unsafe { version_fn(&mut version) }) {
        tracing::info!("pawnio_version failed, defaulting to version 0");
        version = 0;
    }

    let mut handle: HANDLE = std::ptr::null_mut();
    let hr = unsafe { open_fn(&mut handle) };
    if hresult_failed(hr) {
        return Err(SuperIoStatus::PawnIoOpenFailed(hr));
    }

    let hr = unsafe { load_fn(handle, LPCIO_BIN.as_ptr(), LPCIO_BIN.len()) };
    if hresult_failed(hr) {
        unsafe {
            close_fn(handle);
        }
        return Err(SuperIoStatus::ModuleLoadFailed(hr));
    }

    let mutex_name = wide(ISA_MUTEX_NAME);
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if mutex.is_null() {
        let code = unsafe { GetLastError() };
        unsafe {
            close_fn(handle);
        }
        return Err(SuperIoStatus::Io(format!(
            "CreateMutexW failed (error {code})"
        )));
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        tracing::debug!("isa bus mutex already existed, sharing with another driver client");
    }

    let major = (version >> 16) & 0xFF;
    let minor = (version >> 8) & 0xFF;
    let patch = version & 0xFF;
    tracing::info!(
        "pawnio ready version={major}.{minor}.{patch} module=LpcIO {PAWNIO_MODULES_VERSION}"
    );

    Ok(PawnIoTransport {
        _library: library,
        execute: execute_fn,
        close: close_fn,
        handle,
        mutex,
        version,
    })
}

impl PawnIoTransport {
    pub fn version(&self) -> u32 {
        self.version
    }

    fn execute(
        &mut self,
        name: &std::ffi::CStr,
        input: &[u64],
        out_len: usize,
    ) -> Result<Vec<u64>, String> {
        let mut out = vec![0u64; out_len];
        let mut return_size: usize = 0;
        let hr = unsafe {
            (self.execute)(
                self.handle,
                name.as_ptr(),
                input.as_ptr(),
                input.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut return_size,
            )
        };
        if hresult_failed(hr) {
            return Err(hresult_message(
                name.to_str().unwrap_or("pawnio_execute"),
                hr,
            ));
        }
        out.truncate(return_size);
        Ok(out)
    }
}

impl PortIo for PawnIoTransport {
    fn select_slot(&mut self, slot: u8) -> Result<(), String> {
        self.execute(c"ioctl_select_slot", &[slot as u64], 0)
            .map(|_| ())
    }

    fn find_bars(&mut self) -> Result<(), String> {
        self.execute(c"ioctl_find_bars", &[], 0).map(|_| ())
    }

    fn pio_inb(&mut self, port: u16) -> Result<u8, String> {
        let out = self.execute(c"ioctl_pio_inb", &[port as u64], 1)?;
        out.first()
            .copied()
            .map(|v| v as u8)
            .ok_or_else(|| "short read".to_string())
    }

    fn pio_outb(&mut self, port: u16, value: u8) -> Result<(), String> {
        self.execute(c"ioctl_pio_outb", &[port as u64, value as u64], 0)
            .map(|_| ())
    }

    fn superio_inb(&mut self, reg: u8) -> Result<u8, String> {
        let out = self.execute(c"ioctl_superio_inb", &[reg as u64], 1)?;
        out.first()
            .copied()
            .map(|v| v as u8)
            .ok_or_else(|| "short read".to_string())
    }

    fn superio_inw(&mut self, reg: u8) -> Result<u16, String> {
        let out = self.execute(c"ioctl_superio_inw", &[reg as u64], 1)?;
        out.first()
            .copied()
            .map(|v| v as u16)
            .ok_or_else(|| "short read".to_string())
    }

    fn superio_outb(&mut self, reg: u8, value: u8) -> Result<(), String> {
        self.execute(c"ioctl_superio_outb", &[reg as u64, value as u64], 0)
            .map(|_| ())
    }

    fn lock(&mut self, timeout: Duration) -> Result<bool, String> {
        let code = unsafe { WaitForSingleObject(self.mutex, duration_to_wait_ms(timeout)) };
        wait_outcome(code)
    }

    fn unlock(&mut self) {
        if unsafe { ReleaseMutex(self.mutex) } == 0 {
            tracing::debug!("ReleaseMutex failed for isa bus mutex");
        }
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

impl Drop for PawnIoTransport {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.handle);
            CloseHandle(self.mutex);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_reports_missing_or_ready() {
        assert!(matches!(
            open(),
            Ok(_)
                | Err(SuperIoStatus::PawnIoMissing)
                | Err(SuperIoStatus::PawnIoOpenFailed(_))
                | Err(SuperIoStatus::LibraryLoadFailed(_))
        ));
    }
}
