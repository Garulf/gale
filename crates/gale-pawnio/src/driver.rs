use std::ffi::{c_char, CStr, OsString};
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY,
};

use crate::error::{hresult_failed, hresult_message, PawnIoError};
use crate::modules::{ModuleBlob, PAWNIO_MODULES_VERSION};

pub const LIBRARY_NAME: &str = "PawnIOLib.dll";
pub const LIBRARY_ENV_OVERRIDE: &str = "GALE_PAWNIOLIB";
pub const UNINSTALL_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PawnIO";

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

pub(crate) fn wide(value: &str) -> Vec<u16> {
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

pub struct Module {
    _library: libloading::Library,
    execute: PawnioExecute,
    close: PawnioClose,
    handle: HANDLE,
    version: u32,
    name: &'static str,
}

unsafe impl Send for Module {}

impl Module {
    pub fn load(blob: &ModuleBlob) -> Result<Module, PawnIoError> {
        if !is_installed() {
            return Err(PawnIoError::NotInstalled);
        }
        let library = load_library().map_err(PawnIoError::LibraryLoad)?;
        let symbol = |name: &[u8]| -> Result<_, PawnIoError> {
            unsafe { library.get::<*const ()>(name) }
                .map(|s| *s)
                .map_err(|err| PawnIoError::LibraryLoad(err.to_string()))
        };
        let version_fn: PawnioVersion =
            unsafe { std::mem::transmute(symbol(b"pawnio_version\0")?) };
        let open_fn: PawnioOpen = unsafe { std::mem::transmute(symbol(b"pawnio_open\0")?) };
        let load_fn: PawnioLoad = unsafe { std::mem::transmute(symbol(b"pawnio_load\0")?) };
        let execute_fn: PawnioExecute =
            unsafe { std::mem::transmute(symbol(b"pawnio_execute\0")?) };
        let close_fn: PawnioClose = unsafe { std::mem::transmute(symbol(b"pawnio_close\0")?) };

        let mut version = 0u32;
        if hresult_failed(unsafe { version_fn(&mut version) }) {
            tracing::info!("pawnio_version failed, defaulting to version 0");
            version = 0;
        }

        let mut handle: HANDLE = std::ptr::null_mut();
        let hr = unsafe { open_fn(&mut handle) };
        if hresult_failed(hr) {
            return Err(PawnIoError::Open(hr));
        }
        let hr = unsafe { load_fn(handle, blob.bytes.as_ptr(), blob.bytes.len()) };
        if hresult_failed(hr) {
            unsafe {
                close_fn(handle);
            }
            return Err(PawnIoError::ModuleLoad(blob.name, hr));
        }
        let major = (version >> 16) & 0xFF;
        let minor = (version >> 8) & 0xFF;
        let patch = version & 0xFF;
        tracing::info!(
            "pawnio ready version={major}.{minor}.{patch} module={} {PAWNIO_MODULES_VERSION}",
            blob.name
        );
        Ok(Module {
            _library: library,
            execute: execute_fn,
            close: close_fn,
            handle,
            version,
            name: blob.name,
        })
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn execute(
        &mut self,
        name: &CStr,
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
        out.truncate(return_size.min(out_len));
        Ok(out)
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.handle);
        }
    }
}
