use std::ffi::c_void;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Performance::{
    PdhAddCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};
use windows_sys::Win32::System::Power::{
    CallNtPowerInformation, ProcessorInformation, PROCESSOR_POWER_INFORMATION,
};
use windows_sys::Win32::System::Threading::{
    GetActiveProcessorCount, GetSystemTimes, ALL_PROCESSOR_GROUPS,
};

use crate::backend::{
    effective_mhz_from_percent, mean_mhz, CpuFreq, CpuSources, CpuTimes, ProcStat,
};

const STATUS_SUCCESS: i32 = 0;
const PDH_SUCCESS: u32 = 0;
const PROCESSOR_PERFORMANCE_COUNTER_PATH: &str =
    "\\Processor Information(_Total)\\% Processor Performance";

pub fn sources() -> CpuSources {
    let clock: Box<dyn CpuFreq> = match PerformanceCounterFrequency::new() {
        Ok(source) => Box::new(source),
        Err(message) => {
            tracing::warn!(
                %message,
                "falling back to CurrentMhz for cpu/clock; on recent Windows with the AMD power \
                 management driver this reads a constant nominal frequency instead of the live one"
            );
            Box::new(ProcessorPowerInformation)
        }
    };
    CpuSources {
        usage: Some(Box::new(SystemTimes)),
        clock: Some(clock),
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
    let across_every_group = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
    (across_every_group as usize).max(1)
}

fn query_processor_power_information() -> Result<Vec<PROCESSOR_POWER_INFORMATION>, String> {
    let count = processor_count();
    let mut cores: Vec<PROCESSOR_POWER_INFORMATION> = vec![unsafe { std::mem::zeroed() }; count];
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
    Ok(cores)
}

fn base_mhz() -> Result<f64, String> {
    let cores = query_processor_power_information()?;
    let max_values: Vec<f64> = cores.iter().map(|core| f64::from(core.MaxMhz)).collect();
    mean_mhz(&max_values).ok_or_else(|| "no core reported a maximum frequency".to_string())
}

pub struct ProcessorPowerInformation;

impl CpuFreq for ProcessorPowerInformation {
    fn read_mhz(&mut self) -> Result<Vec<f64>, String> {
        let cores = query_processor_power_information()?;
        Ok(cores
            .iter()
            .map(|core| f64::from(core.CurrentMhz))
            .collect())
    }
}

fn to_wide_cstring(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct PerformanceCounterFrequency {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    base_mhz: f64,
}

impl PerformanceCounterFrequency {
    pub fn new() -> Result<Self, String> {
        let base_mhz = base_mhz()?;
        let mut query: PDH_HQUERY = std::ptr::null_mut();
        let status = unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) };
        if status != PDH_SUCCESS {
            return Err(format!("PdhOpenQueryW failed: 0x{status:08x}"));
        }
        let path = to_wide_cstring(PROCESSOR_PERFORMANCE_COUNTER_PATH);
        let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
        let status = unsafe { PdhAddCounterW(query, path.as_ptr(), 0, &mut counter) };
        if status != PDH_SUCCESS {
            unsafe { PdhCloseQuery(query) };
            return Err(format!("PdhAddCounterW failed: 0x{status:08x}"));
        }
        Ok(Self {
            query,
            counter,
            base_mhz,
        })
    }
}

impl Drop for PerformanceCounterFrequency {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

// PDH query and counter handles are not tied to the thread that created them; gale polls
// hardware from a single background thread and never shares a handle across threads at once.
unsafe impl Send for PerformanceCounterFrequency {}

impl CpuFreq for PerformanceCounterFrequency {
    fn read_mhz(&mut self) -> Result<Vec<f64>, String> {
        let status = unsafe { PdhCollectQueryData(self.query) };
        if status != PDH_SUCCESS {
            return Err(format!("PdhCollectQueryData failed: 0x{status:08x}"));
        }
        let mut value: PDH_FMT_COUNTERVALUE = unsafe { std::mem::zeroed() };
        let mut counter_type: u32 = 0;
        let status = unsafe {
            PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, &mut counter_type, &mut value)
        };
        if status != PDH_SUCCESS {
            return Err(format!(
                "PdhGetFormattedCounterValue failed: 0x{status:08x}"
            ));
        }
        let percent = unsafe { value.Anonymous.doubleValue };
        let mhz = effective_mhz_from_percent(percent, self.base_mhz).ok_or_else(|| {
            format!(
                "implausible % Processor Performance value: {percent} at base {} MHz",
                self.base_mhz
            )
        })?;
        tracing::debug!(
            percent,
            base_mhz = self.base_mhz,
            mhz,
            "cpu clock from performance counter"
        );
        Ok(vec![mhz])
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
