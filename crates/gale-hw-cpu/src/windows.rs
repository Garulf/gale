use std::ffi::c_void;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Power::{
    CallNtPowerInformation, ProcessorInformation, PROCESSOR_POWER_INFORMATION,
};
use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows_sys::Win32::System::Threading::GetSystemTimes;

use crate::backend::{CpuFreq, CpuSources, CpuTimes, ProcStat};

const STATUS_SUCCESS: i32 = 0;

pub fn sources() -> CpuSources {
    CpuSources {
        usage: Some(Box::new(SystemTimes)),
        clock: Some(Box::new(ProcessorPowerInformation)),
        power: None,
    }
}

fn filetime_ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

pub struct SystemTimes;

impl ProcStat for SystemTimes {
    fn read(&mut self) -> Result<CpuTimes, String> {
        let mut idle = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = idle;
        let mut user = idle;
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return Err("GetSystemTimes failed".to_string());
        }
        Ok(CpuTimes {
            idle: filetime_ticks(idle),
            total: filetime_ticks(kernel) + filetime_ticks(user),
        })
    }
}

fn processor_count() -> usize {
    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    unsafe { GetSystemInfo(&mut info) };
    (info.dwNumberOfProcessors as usize).max(1)
}

pub struct ProcessorPowerInformation;

impl CpuFreq for ProcessorPowerInformation {
    fn read_mhz(&mut self) -> Result<Vec<f64>, String> {
        let count = processor_count();
        let mut cores: Vec<PROCESSOR_POWER_INFORMATION> =
            vec![unsafe { std::mem::zeroed() }; count];
        let bytes = std::mem::size_of::<PROCESSOR_POWER_INFORMATION>() * count;
        let status = unsafe {
            CallNtPowerInformation(
                ProcessorInformation,
                std::ptr::null(),
                0,
                cores.as_mut_ptr() as *mut c_void,
                bytes as u32,
            )
        };
        if status != STATUS_SUCCESS {
            return Err(format!("CallNtPowerInformation failed: 0x{status:08x}"));
        }
        Ok(cores
            .iter()
            .map(|core| f64::from(core.CurrentMhz))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_halves_combine_into_one_tick_count() {
        assert_eq!(
            filetime_ticks(FILETIME {
                dwLowDateTime: 5,
                dwHighDateTime: 1,
            }),
            (1u64 << 32) + 5
        );
    }

    #[test]
    fn the_windows_sources_offer_usage_and_clock_but_no_package_power() {
        let sources = sources();
        assert!(sources.usage.is_some());
        assert!(sources.clock.is_some());
        assert!(sources.power.is_none());
    }
}
