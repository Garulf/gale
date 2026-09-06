use std::time::Duration;

use gale_pawnio::modules::SMBUSPIIX4;
use gale_pawnio::mutex::SMBUS_MUTEX;
use gale_pawnio::{Module, NamedMutex};

use crate::backend::{DimmBackend, Smbus};
use crate::DimmStatus;

const I2C_SMBUS_WRITE: u64 = 0;
const I2C_SMBUS_READ: u64 = 1;
const I2C_SMBUS_BYTE_DATA: u64 = 2;
const I2C_SMBUS_WORD_DATA: u64 = 3;

pub struct PawnIoSmbus {
    module: Module,
    mutex: NamedMutex,
}

impl Smbus for PawnIoSmbus {
    fn select_port(&mut self, port: i64) -> Result<i64, String> {
        let out = self
            .module
            .execute(c"ioctl_piix4_port_sel", &[port as u64], 1)?;
        out.first()
            .map(|v| *v as i64)
            .ok_or_else(|| "short read".to_string())
    }

    fn read_byte_data(&mut self, address: u8, command: u8) -> Result<u8, String> {
        let out = self.module.execute(
            c"ioctl_smbus_xfer",
            &[
                address as u64,
                I2C_SMBUS_READ,
                command as u64,
                I2C_SMBUS_BYTE_DATA,
            ],
            1,
        )?;
        out.first()
            .map(|v| *v as u8)
            .ok_or_else(|| "short read".to_string())
    }

    fn read_word_data(&mut self, address: u8, command: u8) -> Result<u16, String> {
        let out = self.module.execute(
            c"ioctl_smbus_xfer",
            &[
                address as u64,
                I2C_SMBUS_READ,
                command as u64,
                I2C_SMBUS_WORD_DATA,
            ],
            1,
        )?;
        out.first()
            .map(|v| *v as u16)
            .ok_or_else(|| "short read".to_string())
    }

    fn write_byte_data(&mut self, address: u8, command: u8, value: u8) -> Result<(), String> {
        self.module
            .execute(
                c"ioctl_smbus_xfer",
                &[
                    address as u64,
                    I2C_SMBUS_WRITE,
                    command as u64,
                    I2C_SMBUS_BYTE_DATA,
                    value as u64,
                ],
                0,
            )
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

pub fn probe() -> Result<DimmBackend, DimmStatus> {
    let module = Module::load(&SMBUSPIIX4).map_err(DimmStatus::PawnIo)?;
    let mutex = NamedMutex::open(SMBUS_MUTEX).map_err(DimmStatus::Io)?;
    let mut backend = DimmBackend::new(Box::new(PawnIoSmbus { module, mutex }));
    enumerate_with_retries(&mut backend).map_err(DimmStatus::Io)?;
    if backend.modules().is_empty() {
        return Err(DimmStatus::NoModules);
    }
    Ok(backend)
}

const PROBE_ATTEMPTS: usize = 5;
const PROBE_RETRY_DELAY: Duration = Duration::from_millis(200);

fn enumerate_with_retries(backend: &mut dyn gale_hw::Backend) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 1..=PROBE_ATTEMPTS {
        match backend.enumerate() {
            Ok(_) => return Ok(()),
            Err(error) => {
                last = error.to_string();
                tracing::debug!(attempt, %last, "probe attempt failed");
                if attempt < PROBE_ATTEMPTS {
                    std::thread::sleep(PROBE_RETRY_DELAY);
                }
            }
        }
    }
    Err(last)
}
