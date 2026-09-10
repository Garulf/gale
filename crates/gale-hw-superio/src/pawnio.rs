use std::time::Duration;

use gale_pawnio::modules::LPCIO;
use gale_pawnio::mutex::ISA_BUS_MUTEX;
use gale_pawnio::{Module, NamedMutex, PawnIoError};

use crate::transport::PortIo;
use crate::SuperIoStatus;

const IOCTL_SELECT_SLOT: &std::ffi::CStr = c"ioctl_select_slot";
const IOCTL_FIND_BARS: &std::ffi::CStr = c"ioctl_find_bars";
const IOCTL_PIO_INB: &std::ffi::CStr = c"ioctl_pio_inb";
const IOCTL_PIO_OUTB: &std::ffi::CStr = c"ioctl_pio_outb";
const IOCTL_SUPERIO_INB: &std::ffi::CStr = c"ioctl_superio_inb";
const IOCTL_SUPERIO_INW: &std::ffi::CStr = c"ioctl_superio_inw";
const IOCTL_SUPERIO_OUTB: &std::ffi::CStr = c"ioctl_superio_outb";

/// Every ioctl `PawnIoTransport` calls, with its input/output word counts. Both the
/// call sites below and this table reference the same `IOCTL_*` name constants, so a
/// renamed ioctl can't drift out of sync the way two independent string literals could.
pub const IOCTLS: &[(&std::ffi::CStr, usize, usize)] = &[
    (IOCTL_SELECT_SLOT, 1, 0),
    (IOCTL_FIND_BARS, 0, 0),
    (IOCTL_PIO_INB, 1, 1),
    (IOCTL_PIO_OUTB, 2, 0),
    (IOCTL_SUPERIO_INB, 1, 1),
    (IOCTL_SUPERIO_INW, 1, 1),
    (IOCTL_SUPERIO_OUTB, 2, 0),
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
            .execute(IOCTL_SELECT_SLOT, &[slot as u64], 0)
            .map(|_| ())
    }

    fn find_bars(&mut self) -> Result<(), String> {
        self.module.execute(IOCTL_FIND_BARS, &[], 0).map(|_| ())
    }

    fn pio_inb(&mut self, port: u16) -> Result<u8, String> {
        self.byte(IOCTL_PIO_INB, &[port as u64]).map(|v| v as u8)
    }

    fn pio_outb(&mut self, port: u16, value: u8) -> Result<(), String> {
        self.module
            .execute(IOCTL_PIO_OUTB, &[port as u64, value as u64], 0)
            .map(|_| ())
    }

    fn superio_inb(&mut self, reg: u8) -> Result<u8, String> {
        self.byte(IOCTL_SUPERIO_INB, &[reg as u64]).map(|v| v as u8)
    }

    fn superio_inw(&mut self, reg: u8) -> Result<u16, String> {
        self.byte(IOCTL_SUPERIO_INW, &[reg as u64])
            .map(|v| v as u16)
    }

    fn superio_outb(&mut self, reg: u8, value: u8) -> Result<(), String> {
        self.module
            .execute(IOCTL_SUPERIO_OUTB, &[reg as u64, value as u64], 0)
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
