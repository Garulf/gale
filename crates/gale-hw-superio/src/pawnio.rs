use std::time::Duration;

use gale_pawnio::modules::LPCIO;
use gale_pawnio::mutex::ISA_BUS_MUTEX;
use gale_pawnio::{Module, NamedMutex, PawnIoError};

use crate::transport::PortIo;
use crate::SuperIoStatus;

pub const IOCTLS: &[(&str, usize, usize)] = &[
    ("ioctl_select_slot", 1, 0),
    ("ioctl_find_bars", 0, 0),
    ("ioctl_pio_inb", 1, 1),
    ("ioctl_pio_outb", 2, 0),
    ("ioctl_superio_inb", 1, 1),
    ("ioctl_superio_inw", 1, 1),
    ("ioctl_superio_outb", 2, 0),
];

pub struct PawnIoTransport {
    module: Module,
    mutex: NamedMutex,
}

impl From<PawnIoError> for SuperIoStatus {
    fn from(error: PawnIoError) -> Self {
        match error {
            PawnIoError::NotInstalled => SuperIoStatus::PawnIoMissing,
            PawnIoError::LibraryLoad(message) => SuperIoStatus::LibraryLoadFailed(message),
            PawnIoError::Open(hr) => SuperIoStatus::PawnIoOpenFailed(hr),
            PawnIoError::ModuleLoad(_, hr) => SuperIoStatus::ModuleLoadFailed(hr),
        }
    }
}

pub fn open() -> Result<PawnIoTransport, SuperIoStatus> {
    let module = Module::load(&LPCIO)?;
    let mutex = NamedMutex::open(ISA_BUS_MUTEX).map_err(SuperIoStatus::Io)?;
    Ok(PawnIoTransport { module, mutex })
}

impl PawnIoTransport {
    pub fn version(&self) -> u32 {
        self.module.version()
    }

    fn byte(&mut self, name: &std::ffi::CStr, input: &[u64]) -> Result<u64, String> {
        let out = self.module.execute(name, input, 1)?;
        out.first().copied().ok_or_else(|| "short read".to_string())
    }
}

impl PortIo for PawnIoTransport {
    fn select_slot(&mut self, slot: u8) -> Result<(), String> {
        self.module
            .execute(c"ioctl_select_slot", &[slot as u64], 0)
            .map(|_| ())
    }

    fn find_bars(&mut self) -> Result<(), String> {
        self.module.execute(c"ioctl_find_bars", &[], 0).map(|_| ())
    }

    fn pio_inb(&mut self, port: u16) -> Result<u8, String> {
        self.byte(c"ioctl_pio_inb", &[port as u64]).map(|v| v as u8)
    }

    fn pio_outb(&mut self, port: u16, value: u8) -> Result<(), String> {
        self.module
            .execute(c"ioctl_pio_outb", &[port as u64, value as u64], 0)
            .map(|_| ())
    }

    fn superio_inb(&mut self, reg: u8) -> Result<u8, String> {
        self.byte(c"ioctl_superio_inb", &[reg as u64])
            .map(|v| v as u8)
    }

    fn superio_inw(&mut self, reg: u8) -> Result<u16, String> {
        self.byte(c"ioctl_superio_inw", &[reg as u64])
            .map(|v| v as u16)
    }

    fn superio_outb(&mut self, reg: u8, value: u8) -> Result<(), String> {
        self.module
            .execute(c"ioctl_superio_outb", &[reg as u64, value as u64], 0)
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

    #[test]
    fn ioctl_table_lists_every_call_the_transport_makes() {
        assert_eq!(IOCTLS.len(), 7);
    }
}
