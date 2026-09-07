use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::Nvml;
use std::sync::Arc;

pub trait NvmlFacade: Send {
    fn device_count(&self) -> Result<u32, String>;
    fn device(&self, index: u32) -> Result<Box<dyn NvmlDevice>, String>;
}

pub trait NvmlDevice: Send {
    fn name(&self) -> String;
    fn temperature(&self) -> Option<f64>;
    fn fan_count(&self) -> u32;
    fn fan_duty(&self, fan: u32) -> Option<f64>;
    fn fan_rpm(&self, fan: u32) -> Option<f64>;
    fn set_fan_duty(&mut self, fan: u32, pct: f64) -> Result<(), String>;
    fn restore_fan_default(&mut self, fan: u32) -> Result<(), String>;
    fn fan_controllable(&self, fan: u32) -> bool;
    fn min_max_fan_duty(&self, fan: u32) -> Option<(f64, f64)>;
    fn utilization(&self) -> Option<(f64, f64)>;
    fn memory(&self) -> Option<(f64, f64)>;
    fn clocks(&self) -> Option<(f64, f64)>;
    fn power_watts(&self) -> Option<f64>;
    fn performance_state(&self) -> Option<f64>;
}

pub struct RealNvml {
    nvml: Arc<Nvml>,
}

impl RealNvml {
    pub fn try_init() -> Option<RealNvml> {
        Nvml::init().ok().map(|nvml| RealNvml {
            nvml: Arc::new(nvml),
        })
    }
}

impl NvmlFacade for RealNvml {
    fn device_count(&self) -> Result<u32, String> {
        self.nvml.device_count().map_err(|error| error.to_string())
    }

    fn device(&self, index: u32) -> Result<Box<dyn NvmlDevice>, String> {
        self.nvml
            .device_by_index(index)
            .map_err(|error| error.to_string())?;
        Ok(Box::new(RealNvmlDevice {
            nvml: self.nvml.clone(),
            index,
        }))
    }
}

struct RealNvmlDevice {
    nvml: Arc<Nvml>,
    index: u32,
}

impl RealNvmlDevice {
    fn device(&self) -> Option<nvml_wrapper::Device<'_>> {
        self.nvml.device_by_index(self.index).ok()
    }
}

impl NvmlDevice for RealNvmlDevice {
    fn name(&self) -> String {
        self.device()
            .and_then(|device| device.name().ok())
            .unwrap_or_default()
    }

    fn temperature(&self) -> Option<f64> {
        self.device()?
            .temperature(TemperatureSensor::Gpu)
            .ok()
            .map(f64::from)
    }

    fn fan_count(&self) -> u32 {
        self.device()
            .and_then(|device| device.num_fans().ok())
            .unwrap_or(0)
    }

    fn fan_duty(&self, fan: u32) -> Option<f64> {
        self.device()?.fan_speed(fan).ok().map(f64::from)
    }

    fn fan_rpm(&self, fan: u32) -> Option<f64> {
        self.device()?.fan_speed_rpm(fan).ok().map(f64::from)
    }

    fn set_fan_duty(&mut self, fan: u32, pct: f64) -> Result<(), String> {
        let mut device = self
            .nvml
            .device_by_index(self.index)
            .map_err(|error| error.to_string())?;
        device
            .set_fan_speed(fan, pct.round() as u32)
            .map_err(|error| error.to_string())
    }

    fn restore_fan_default(&mut self, fan: u32) -> Result<(), String> {
        let mut device = self
            .nvml
            .device_by_index(self.index)
            .map_err(|error| error.to_string())?;
        device
            .set_default_fan_speed(fan)
            .map_err(|error| error.to_string())
    }

    fn fan_controllable(&self, fan: u32) -> bool {
        self.device()
            .map(|device| device.fan_control_policy(fan).is_ok())
            .unwrap_or(false)
    }

    fn min_max_fan_duty(&self, _fan: u32) -> Option<(f64, f64)> {
        self.device()?
            .min_max_fan_speed()
            .ok()
            .map(|(min, max)| (f64::from(min), f64::from(max)))
    }

    fn utilization(&self) -> Option<(f64, f64)> {
        let rates = self.device()?.utilization_rates().ok()?;
        Some((f64::from(rates.gpu), f64::from(rates.memory)))
    }

    fn memory(&self) -> Option<(f64, f64)> {
        let info = self.device()?.memory_info().ok()?;
        const MIB: f64 = 1024.0 * 1024.0;
        Some((info.used as f64 / MIB, info.total as f64 / MIB))
    }

    fn clocks(&self) -> Option<(f64, f64)> {
        let device = self.device()?;
        let graphics = device.clock_info(Clock::Graphics).ok()?;
        let memory = device.clock_info(Clock::Memory).ok()?;
        Some((f64::from(graphics), f64::from(memory)))
    }

    fn power_watts(&self) -> Option<f64> {
        self.device()?
            .power_usage()
            .ok()
            .map(|mw| f64::from(mw) / 1000.0)
    }

    fn performance_state(&self) -> Option<f64> {
        use nvml_wrapper::enum_wrappers::device::PerformanceState as P;
        let state = self.device()?.performance_state().ok()?;
        let n = match state {
            P::Zero => 0,
            P::One => 1,
            P::Two => 2,
            P::Three => 3,
            P::Four => 4,
            P::Five => 5,
            P::Six => 6,
            P::Seven => 7,
            P::Eight => 8,
            P::Nine => 9,
            P::Ten => 10,
            P::Eleven => 11,
            P::Twelve => 12,
            P::Thirteen => 13,
            P::Fourteen => 14,
            P::Fifteen => 15,
            P::Unknown => return None,
        };
        Some(f64::from(n))
    }
}
