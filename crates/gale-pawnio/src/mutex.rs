pub const ISA_BUS_MUTEX: &str = "Global\\Access_ISABUS.HTP.Method";
pub const PCI_BUS_MUTEX: &str = "Global\\Access_PCI";
pub const SMBUS_MUTEX: &str = "Global\\Access_SMBUS.HTP.Method";
pub const EC_MUTEX: &str = "Global\\Access_EC";

pub const WAIT_OBJECT_0: u32 = 0x0000_0000;
pub const WAIT_ABANDONED: u32 = 0x0000_0080;
pub const WAIT_TIMEOUT: u32 = 0x0000_0102;
pub const WAIT_FAILED: u32 = 0xFFFF_FFFF;

pub fn wait_outcome(code: u32) -> Result<bool, String> {
    match code {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err("WaitForSingleObject failed".to_string()),
        other => Err(format!("unexpected wait result 0x{other:x}")),
    }
}

pub fn duration_to_wait_ms(d: std::time::Duration) -> u32 {
    let ms = d.as_millis();
    if ms >= u32::MAX as u128 {
        u32::MAX - 1
    } else {
        ms as u32
    }
}

#[cfg(windows)]
mod imp {
    use super::{duration_to_wait_ms, wait_outcome};
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

    pub struct NamedMutex {
        handle: HANDLE,
    }

    unsafe impl Send for NamedMutex {}

    impl NamedMutex {
        pub fn open(name: &str) -> Result<Self, String> {
            let wide = crate::driver::wide(name);
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
            if handle.is_null() {
                let code = unsafe { GetLastError() };
                return Err(format!("CreateMutexW failed (error {code})"));
            }
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                tracing::debug!(
                    name,
                    "mutex already existed, sharing with another driver client"
                );
            }
            Ok(Self { handle })
        }

        pub fn lock(&self, timeout: Duration) -> Result<bool, String> {
            let code = unsafe { WaitForSingleObject(self.handle, duration_to_wait_ms(timeout)) };
            wait_outcome(code)
        }

        pub fn unlock(&self) {
            if unsafe { ReleaseMutex(self.handle) } == 0 {
                tracing::debug!("ReleaseMutex failed");
            }
        }

        pub fn guard(&self, timeout: Duration) -> Result<Option<MutexGuard<'_>>, String> {
            if self.lock(timeout)? {
                Ok(Some(MutexGuard { mutex: self }))
            } else {
                Ok(None)
            }
        }
    }

    impl Drop for NamedMutex {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    pub struct MutexGuard<'a> {
        mutex: &'a NamedMutex,
    }

    impl Drop for MutexGuard<'_> {
        fn drop(&mut self) {
            self.mutex.unlock();
        }
    }
}

#[cfg(windows)]
pub use imp::{MutexGuard, NamedMutex};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_outcome_maps_codes() {
        assert_eq!(wait_outcome(WAIT_OBJECT_0), Ok(true));
        assert_eq!(wait_outcome(WAIT_ABANDONED), Ok(true));
        assert_eq!(wait_outcome(WAIT_TIMEOUT), Ok(false));
        assert!(wait_outcome(WAIT_FAILED).is_err());
        assert!(wait_outcome(7).is_err());
    }

    #[test]
    fn duration_to_wait_ms_saturates() {
        assert_eq!(
            duration_to_wait_ms(std::time::Duration::from_millis(250)),
            250
        );
        assert_eq!(
            duration_to_wait_ms(std::time::Duration::from_secs(u64::MAX / 1000)),
            u32::MAX - 1
        );
    }
}
