pub mod backend;
pub mod commander_pro;
pub mod transport;

use std::collections::HashMap;

pub trait CorsairDevice: Send {
    fn slug(&self) -> &str;
    fn channels(&mut self) -> Result<DeviceChannels, String>;
    fn read(&mut self) -> HashMap<String, Option<f64>>;
    fn set_duty(&mut self, channel: &str, pct: f64) -> Result<(), String>;
    fn release(&mut self, channel: &str) -> Result<(), String>;
}

pub struct DeviceChannels {
    pub sensors: Vec<(String, gale_hw::SensorKind, String)>,
    pub controls: Vec<(String, String)>,
}

pub use backend::CorsairBackend;
