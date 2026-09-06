use std::collections::HashMap;
use std::time::Duration;

use gale_hw::{Backend, HwError, Id, Inventory, SensorInfo, SensorKind};

use crate::spd::*;

pub const SMBUS_LOCK_TIMEOUT: Duration = Duration::from_millis(250);
pub const IO_DELAY: Duration = Duration::from_millis(1);
pub const DATA_RETRIES: usize = 5;
pub const TEMPERATURE_RETRIES: usize = 12;
pub const PORTS: [i64; 2] = [0, 1];
pub const PREFIX: &str = "dimm";

pub trait Smbus: Send {
    fn select_port(&mut self, port: i64) -> Result<i64, String>;
    fn read_byte_data(&mut self, address: u8, command: u8) -> Result<u8, String>;
    fn read_word_data(&mut self, address: u8, command: u8) -> Result<u16, String>;
    fn write_byte_data(&mut self, address: u8, command: u8, value: u8) -> Result<(), String>;
    fn lock(&mut self, timeout: Duration) -> Result<bool, String>;
    fn unlock(&mut self);
    fn sleep(&mut self, duration: Duration);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleKind {
    Ddr4 { thermal_sensor: u8 },
    Ddr5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimmModule {
    pub port: i64,
    pub address: u8,
    pub kind: ModuleKind,
}

impl DimmModule {
    pub fn slot(&self) -> u8 {
        self.address - SPD_FIRST + 1
    }

    pub fn id(&self) -> Id {
        if self.port == 0 {
            format!("{PREFIX}/{}/temp", self.slot())
        } else {
            format!("{PREFIX}/{}-bus{}/temp", self.slot(), self.port)
        }
    }

    pub fn label(&self) -> String {
        let kind = match self.kind {
            ModuleKind::Ddr4 { .. } => "DDR4",
            ModuleKind::Ddr5 => "DDR5",
        };
        if self.port == 0 {
            format!("DIMM {} ({kind})", self.slot())
        } else {
            format!("DIMM {} ({kind}, bus {})", self.slot(), self.port)
        }
    }
}

fn retry<T>(
    io: &mut dyn Smbus,
    retries: usize,
    mut op: impl FnMut(&mut dyn Smbus) -> Result<T, String>,
) -> Result<T, String> {
    let mut last = String::new();
    for attempt in 0..retries {
        match op(io) {
            Ok(value) => return Ok(value),
            Err(error) => {
                last = error;
                if attempt + 1 < retries {
                    io.sleep(IO_DELAY);
                }
            }
        }
    }
    Err(last)
}

fn ddr4_select_page0(io: &mut dyn Smbus) {
    let _ = io.write_byte_data(DDR4_PAGE_ADDRESS, 0x00, 0xFF);
    io.sleep(IO_DELAY);
}

fn probe_address(io: &mut dyn Smbus, port: i64, address: u8) -> Option<DimmModule> {
    ddr4_select_page0(io);
    if let Ok(memory_type) = retry(io, DATA_RETRIES, |io| {
        io.read_byte_data(address, DDR4_MEMORY_TYPE_BYTE)
    }) {
        if is_ddr4_type(memory_type) {
            let thermal_sensor = ddr4_thermal_sensor_address(address);
            let advertised = retry(io, DATA_RETRIES, |io| {
                io.read_byte_data(address, DDR4_THERMAL_SENSOR_BYTE)
            })
            .map(|byte| byte & DDR4_THERMAL_SENSOR_BIT != 0)
            .unwrap_or(false);
            let has_sensor = advertised
                || retry(io, DATA_RETRIES, |io| {
                    io.read_word_data(thermal_sensor, DDR4_TS_CAPABILITIES_REGISTER)
                })
                .is_ok();
            if !has_sensor {
                tracing::info!(
                    port,
                    address = format!("0x{address:02x}"),
                    "ddr4 module without a thermal sensor"
                );
                return None;
            }
            return Some(DimmModule {
                port,
                address,
                kind: ModuleKind::Ddr4 { thermal_sensor },
            });
        }
    }
    let msb = retry(io, DATA_RETRIES, |io| {
        io.read_byte_data(address, DDR5_DEVICE_TYPE_MSB)
    });
    let lsb = retry(io, DATA_RETRIES, |io| {
        io.read_byte_data(address, DDR5_DEVICE_TYPE_LSB)
    });
    tracing::debug!(
        port,
        address = format!("0x{address:02x}"),
        ?msb,
        ?lsb,
        "spd5 device type"
    );
    let (msb, lsb) = (msb.ok()?, lsb.ok()?);
    if msb != DDR5_DEVICE_TYPE_MSB_EXPECTED || lsb != DDR5_DEVICE_TYPE_LSB_EXPECTED {
        return None;
    }
    let disabled = retry(io, DATA_RETRIES, |io| {
        io.read_byte_data(address, DDR5_THERMAL_SENSOR_DISABLED)
    })
    .ok()?;
    if disabled != 0 {
        tracing::info!(
            port,
            address = format!("0x{address:02x}"),
            "ddr5 module thermal sensor disabled"
        );
        return None;
    }
    Some(DimmModule {
        port,
        address,
        kind: ModuleKind::Ddr5,
    })
}

fn with_port<T>(
    io: &mut dyn Smbus,
    port: i64,
    f: impl FnOnce(&mut dyn Smbus) -> T,
) -> Result<T, String> {
    let previous = io.select_port(port)?;
    let result = f(io);
    if previous != port {
        io.select_port(previous)?;
    }
    Ok(result)
}

pub fn detect(io: &mut dyn Smbus) -> Result<Vec<DimmModule>, String> {
    let mut modules = Vec::new();
    for port in PORTS {
        tracing::debug!(port, "probing piix4 smbus port");
        let found = with_port(io, port, |io| {
            (SPD_FIRST..=SPD_LAST)
                .filter_map(|address| probe_address(io, port, address))
                .collect::<Vec<_>>()
        })?;
        modules.extend(found);
    }
    Ok(modules)
}

fn read_temperature(io: &mut dyn Smbus, module: &DimmModule) -> Option<f64> {
    with_port(io, module.port, |io| match module.kind {
        ModuleKind::Ddr4 { thermal_sensor } => {
            ddr4_select_page0(io);
            retry(io, TEMPERATURE_RETRIES, |io| {
                io.read_word_data(thermal_sensor, DDR4_TS_TEMPERATURE_REGISTER)
            })
            .ok()
            .map(|word| temperature_from_raw(swap_bytes(word)))
        }
        ModuleKind::Ddr5 => retry(io, TEMPERATURE_RETRIES, |io| {
            io.read_word_data(module.address, DDR5_TEMPERATURE_REGISTER)
        })
        .ok()
        .map(temperature_from_raw),
    })
    .ok()
    .flatten()
}

pub struct DimmBackend {
    io: Box<dyn Smbus>,
    modules: Vec<DimmModule>,
}

impl DimmBackend {
    pub fn new(io: Box<dyn Smbus>) -> Self {
        Self {
            io,
            modules: Vec::new(),
        }
    }

    pub fn modules(&self) -> &[DimmModule] {
        &self.modules
    }

    fn locked<T>(&mut self, f: impl FnOnce(&mut dyn Smbus) -> T) -> Result<T, String> {
        if !self.io.lock(SMBUS_LOCK_TIMEOUT)? {
            return Err("smbus lock timed out".to_string());
        }
        let result = f(self.io.as_mut());
        self.io.unlock();
        Ok(result)
    }
}

impl Backend for DimmBackend {
    fn name(&self) -> &str {
        "dimm"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        let modules = self
            .locked(|io| detect(io))
            .and_then(|r| r)
            .map_err(|message| HwError::Io {
                path: PREFIX.to_string(),
                message,
            })?;
        self.modules = modules;
        Ok(Inventory {
            sensors: self
                .modules
                .iter()
                .map(|module| SensorInfo {
                    id: module.id(),
                    label: module.label(),
                    kind: SensorKind::Temp,
                })
                .collect(),
            controls: Vec::new(),
        })
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let modules = self.modules.clone();
        let mut values: HashMap<Id, Option<f64>> = modules.iter().map(|m| (m.id(), None)).collect();
        if let Ok(read) = self.locked(|io| {
            modules
                .iter()
                .map(|m| (m.id(), read_temperature(io, m)))
                .collect::<Vec<_>>()
        }) {
            values.extend(read);
        }
        values
    }

    fn set_duty(&mut self, id: &str, _pct: f64) -> Result<(), HwError> {
        Err(HwError::UnknownId(id.to_string()))
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        Err(HwError::UnknownId(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeBus {
        port: i64,
        bytes: HashMap<(i64, u8, u8), u8>,
        words: HashMap<(i64, u8, u8), u16>,
        busy_first: HashMap<(i64, u8, u8), usize>,
        log: Vec<String>,
        locked: bool,
    }

    impl FakeBus {
        fn ddr4_module(&mut self, port: i64, address: u8, temp_word_swapped: u16) {
            self.bytes
                .insert((port, address, DDR4_MEMORY_TYPE_BYTE), 12);
            self.bytes
                .insert((port, address, DDR4_THERMAL_SENSOR_BYTE), 0x80);
            self.words.insert(
                (
                    port,
                    ddr4_thermal_sensor_address(address),
                    DDR4_TS_TEMPERATURE_REGISTER,
                ),
                temp_word_swapped,
            );
        }
        fn ddr5_module(&mut self, port: i64, address: u8, temp_word: u16) {
            self.bytes
                .insert((port, address, DDR5_DEVICE_TYPE_MSB), 0x51);
            self.bytes
                .insert((port, address, DDR5_DEVICE_TYPE_LSB), 0x18);
            self.bytes
                .insert((port, address, DDR5_THERMAL_SENSOR_DISABLED), 0);
            self.words
                .insert((port, address, DDR5_TEMPERATURE_REGISTER), temp_word);
        }
    }

    impl Smbus for FakeBus {
        fn select_port(&mut self, port: i64) -> Result<i64, String> {
            self.log.push(format!("port {port}"));
            let previous = self.port;
            self.port = port;
            Ok(previous)
        }
        fn read_byte_data(&mut self, address: u8, command: u8) -> Result<u8, String> {
            assert!(self.locked);
            self.log.push(format!("rb {:x} {:x}", address, command));
            self.bytes
                .get(&(self.port, address, command))
                .copied()
                .ok_or_else(|| "nak".into())
        }
        fn read_word_data(&mut self, address: u8, command: u8) -> Result<u16, String> {
            assert!(self.locked);
            self.log.push(format!("rw {:x} {:x}", address, command));
            let key = (self.port, address, command);
            if let Some(left) = self.busy_first.get_mut(&key) {
                if *left > 0 {
                    *left -= 1;
                    return Err("busy".into());
                }
            }
            self.words.get(&key).copied().ok_or_else(|| "nak".into())
        }
        fn write_byte_data(&mut self, address: u8, command: u8, value: u8) -> Result<(), String> {
            self.log
                .push(format!("wb {:x} {:x} {:x}", address, command, value));
            Ok(())
        }
        fn lock(&mut self, _timeout: Duration) -> Result<bool, String> {
            self.locked = true;
            Ok(true)
        }
        fn unlock(&mut self) {
            self.locked = false;
        }
        fn sleep(&mut self, _duration: Duration) {}
    }

    #[test]
    fn detects_ddr4_modules_with_sensors_and_skips_the_rest() {
        let mut bus = FakeBus::default();
        bus.ddr4_module(0, 0x50, 0x8002);
        bus.ddr4_module(0, 0x51, 0x8002);
        bus.bytes.insert((0, 0x52, DDR4_MEMORY_TYPE_BYTE), 12);
        bus.bytes.insert((0, 0x52, DDR4_THERMAL_SENSOR_BYTE), 0x00);
        bus.locked = true;
        let modules = detect(&mut bus).unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].id(), "dimm/1/temp");
        assert_eq!(modules[1].label(), "DIMM 2 (DDR4)");
        assert!(matches!(
            modules[0].kind,
            ModuleKind::Ddr4 {
                thermal_sensor: 0x18
            }
        ));
        assert!(
            bus.log.contains(&"wb 36 0 ff".to_string()),
            "page 0 must be selected before probing"
        );
    }

    #[test]
    fn a_module_that_does_not_advertise_its_sensor_is_found_by_probing_the_sensor_address() {
        let mut bus = FakeBus {
            locked: true,
            ..Default::default()
        };
        bus.bytes.insert((0, 0x50, DDR4_MEMORY_TYPE_BYTE), 12);
        bus.bytes.insert((0, 0x50, DDR4_THERMAL_SENSOR_BYTE), 0x00);
        bus.words
            .insert((0, 0x18, DDR4_TS_CAPABILITIES_REGISTER), 0x00E7);
        let modules = detect(&mut bus).unwrap();
        assert_eq!(modules.len(), 1);
        assert!(matches!(
            modules[0].kind,
            ModuleKind::Ddr4 {
                thermal_sensor: 0x18
            }
        ));
    }

    #[test]
    fn detects_ddr5_modules_by_device_type_and_enabled_sensor() {
        let mut bus = FakeBus::default();
        bus.ddr5_module(1, 0x53, 0x0280);
        bus.locked = true;
        let modules = detect(&mut bus).unwrap();
        assert_eq!(
            modules,
            vec![DimmModule {
                port: 1,
                address: 0x53,
                kind: ModuleKind::Ddr5
            }]
        );
        assert_eq!(modules[0].label(), "DIMM 4 (DDR5, bus 1)");
        assert_eq!(modules[0].id(), "dimm/4-bus1/temp");
    }

    #[test]
    fn every_port_selection_is_restored_afterwards() {
        let mut bus = FakeBus {
            locked: true,
            ..Default::default()
        };
        detect(&mut bus).unwrap();
        let ports: Vec<&String> = bus.log.iter().filter(|l| l.starts_with("port")).collect();
        assert_eq!(ports, vec!["port 0", "port 1", "port 0"]);
    }

    #[test]
    fn read_all_swaps_ddr4_words_and_retries_busy_reads() {
        let mut bus = FakeBus::default();
        bus.ddr4_module(0, 0x50, 0x8002);
        bus.ddr5_module(0, 0x51, 0x0280);
        bus.busy_first
            .insert((0, 0x18, DDR4_TS_TEMPERATURE_REGISTER), 3);
        let mut backend = DimmBackend::new(Box::new(bus));
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["dimm/1/temp"], Some(40.0));
        assert_eq!(values["dimm/2/temp"], Some(40.0));
    }

    #[test]
    fn a_module_that_stops_answering_reads_none_without_dropping_the_others() {
        let mut bus = FakeBus::default();
        bus.ddr4_module(0, 0x50, 0x8002);
        bus.ddr4_module(0, 0x51, 0x8002);
        let mut backend = DimmBackend::new(Box::new(bus));
        backend.enumerate().unwrap();
        let mut broken = FakeBus::default();
        broken.ddr4_module(0, 0x50, 0x8002);
        backend.io = Box::new(broken);
        let values = backend.read_all();
        assert_eq!(values["dimm/1/temp"], Some(40.0));
        assert_eq!(values["dimm/2/temp"], None);
    }
}
