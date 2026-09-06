use std::time::Duration;

use gale_hw::Backend;

use gale_pawnio::modules::LPCACPIEC;
use gale_pawnio::mutex::EC_MUTEX;
use gale_pawnio::{Module, NamedMutex};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    KEY_WOW64_64KEY,
};

use crate::backend::AsusEcBackend;
use crate::boards::find_board;
use crate::protocol::EcPorts;
use crate::EcStatus;

const BIOS_KEY: &str = "HARDWARE\\DESCRIPTION\\System\\BIOS";

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn registry_string(value_name: &str) -> Option<String> {
    let key_path = wide(BIOS_KEY);
    let mut hkey: HKEY = std::ptr::null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            key_path.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        )
    };
    if opened != 0 {
        return None;
    }
    let name = wide(value_name);
    let mut buffer = vec![0u16; 256];
    let mut size = (buffer.len() * 2) as u32;
    let result = unsafe {
        RegQueryValueExW(
            hkey,
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as *mut u8,
            &mut size,
        )
    };
    unsafe {
        RegCloseKey(hkey);
    }
    if result != 0 {
        return None;
    }
    let chars = (size as usize / 2).min(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..chars]);
    Some(text.trim_end_matches('\0').trim().to_string())
}

pub struct PawnIoEc {
    module: Module,
    mutex: NamedMutex,
}

impl EcPorts for PawnIoEc {
    fn read_port(&mut self, port: u8) -> Result<u8, String> {
        let out = self.module.execute(c"ioctl_pio_read", &[port as u64], 1)?;
        out.first()
            .map(|v| *v as u8)
            .ok_or_else(|| "short read".to_string())
    }

    fn write_port(&mut self, port: u8, value: u8) -> Result<(), String> {
        self.module
            .execute(c"ioctl_pio_write", &[port as u64, value as u64], 0)
            .map(|_| ())
    }

    fn lock(&mut self, timeout: Duration) -> Result<bool, String> {
        self.mutex.lock(timeout)
    }

    fn unlock(&mut self) {
        self.mutex.unlock();
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

pub fn probe() -> Result<AsusEcBackend, EcStatus> {
    let manufacturer = registry_string("BaseBoardManufacturer").unwrap_or_default();
    if !manufacturer.to_ascii_uppercase().contains("ASUS") {
        return Err(EcStatus::NotAsus(manufacturer));
    }
    let product = registry_string("BaseBoardProduct").unwrap_or_default();
    let board = find_board(&product).ok_or_else(|| EcStatus::UnknownBoard(product.clone()))?;
    let module = Module::load(&LPCACPIEC).map_err(EcStatus::PawnIo)?;
    let mutex = NamedMutex::open(EC_MUTEX).map_err(EcStatus::Io)?;
    let mut backend = AsusEcBackend::new(Box::new(PawnIoEc { module, mutex }), board);
    backend
        .enumerate()
        .map_err(|e| EcStatus::Io(e.to_string()))?;
    tracing::info!(
        product,
        sensors = backend.read_all().len(),
        "asus embedded controller ready"
    );
    Ok(backend)
}
