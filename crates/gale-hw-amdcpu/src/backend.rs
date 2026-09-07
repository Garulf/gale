use std::collections::HashMap;
use std::time::Duration;

use std::time::Instant;

use gale_hw::rate::{per_second, plausible_watts, Counter};
use gale_hw::{Backend, HwError, Id, Inventory, SensorInfo, SensorKind};

use crate::decode::{
    brand_tctl_offset, ccd_register, ccd_temp_from_raw, energy_unit_joules, package_energy_counter,
    supports_per_ccd, tctl_from_raw, ENERGY_COUNTER_MODULUS, MAX_CCDS, MSR_PKG_ENERGY_STAT,
    MSR_RAPL_PWR_UNIT, THM_TCON_CUR_TMP,
};

pub const PCI_LOCK_TIMEOUT: Duration = Duration::from_millis(10);
pub const PREFIX: &str = "cpu/amd";

pub trait CpuIo: Send {
    fn read_smn(&mut self, address: u32) -> Result<u32, String>;
    fn read_msr(&mut self, msr: u32) -> Result<u64, String>;
    fn lock(&mut self, timeout: Duration) -> Result<bool, String>;
    fn unlock(&mut self);
}

pub struct AmdCpuBackend {
    io: Box<dyn CpuIo>,
    model: u32,
    tctl_offset: f64,
    ccds: Vec<u32>,
    energy_unit: Option<f64>,
    previous_energy: Option<Counter>,
    clock: Box<dyn FnMut() -> Instant + Send>,
}

fn id(name: &str) -> Id {
    format!("{PREFIX}/{name}")
}

fn ccd_id(ccd: u32) -> Id {
    id(&format!("ccd{}", ccd + 1))
}

impl AmdCpuBackend {
    pub fn new(io: Box<dyn CpuIo>, model: u32, brand: &str) -> Self {
        Self {
            io,
            model,
            tctl_offset: brand_tctl_offset(brand),
            ccds: Vec::new(),
            energy_unit: None,
            previous_energy: None,
            clock: Box::new(Instant::now),
        }
    }

    pub fn with_clock(mut self, clock: Box<dyn FnMut() -> Instant + Send>) -> Self {
        self.clock = clock;
        self
    }

    fn with_lock<T>(&mut self, f: impl FnOnce(&mut dyn CpuIo) -> T) -> Result<T, String> {
        if !self.io.lock(PCI_LOCK_TIMEOUT)? {
            return Err("pci bus lock timed out".to_string());
        }
        let result = f(self.io.as_mut());
        self.io.unlock();
        Ok(result)
    }
}

impl Backend for AmdCpuBackend {
    fn name(&self) -> &str {
        "amdcpu"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        let model = self.model;
        let probe = self
            .with_lock(|io| {
                let tctl = io.read_smn(THM_TCON_CUR_TMP)?;
                let mut ccds = Vec::new();
                if supports_per_ccd(model) {
                    for ccd in 0..MAX_CCDS {
                        if ccd_temp_from_raw(io.read_smn(ccd_register(model, ccd))?).is_some() {
                            ccds.push(ccd);
                        }
                    }
                }
                let energy_unit = io
                    .read_msr(MSR_RAPL_PWR_UNIT)
                    .and_then(|raw| {
                        io.read_msr(MSR_PKG_ENERGY_STAT)?;
                        energy_unit_joules(raw)
                            .ok_or_else(|| format!("implausible energy unit 0x{raw:x}"))
                    })
                    .map_err(|message| tracing::debug!(%message, "amd package energy unavailable"))
                    .ok();
                Ok::<_, String>((tctl, ccds, energy_unit))
            })
            .and_then(|r| r)
            .map_err(|message| HwError::Io {
                path: PREFIX.to_string(),
                message,
            })?;
        self.ccds = probe.1;
        self.energy_unit = probe.2;
        tracing::info!(
            tctl = tctl_from_raw(probe.0),
            ccds = self.ccds.len(),
            model = format!("0x{:x}", self.model),
            "amd cpu thermal registers readable"
        );
        let mut sensors = vec![SensorInfo {
            id: id("tctl"),
            label: if self.tctl_offset < 0.0 {
                "CPU Tctl"
            } else {
                "CPU Tctl/Tdie"
            }
            .to_string(),
            kind: SensorKind::Temp,
        }];
        if self.tctl_offset < 0.0 {
            sensors.push(SensorInfo {
                id: id("tdie"),
                label: "CPU Tdie".to_string(),
                kind: SensorKind::Temp,
            });
        }
        for ccd in &self.ccds {
            sensors.push(SensorInfo {
                id: ccd_id(*ccd),
                label: format!("CPU CCD{}", ccd + 1),
                kind: SensorKind::Temp,
            });
        }
        if self.energy_unit.is_some() {
            sensors.push(SensorInfo {
                id: id("power"),
                label: "CPU Package Power".to_string(),
                kind: SensorKind::Power,
            });
        }
        Ok(Inventory {
            sensors,
            controls: Vec::new(),
        })
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let model = self.model;
        let ccds = self.ccds.clone();
        let offset = self.tctl_offset;
        let energy_unit = self.energy_unit;
        let mut values: HashMap<Id, Option<f64>> = HashMap::new();
        values.insert(id("tctl"), None);
        if energy_unit.is_some() {
            values.insert(id("power"), None);
        }
        if offset < 0.0 {
            values.insert(id("tdie"), None);
        }
        for ccd in &ccds {
            values.insert(ccd_id(*ccd), None);
        }
        let read = self.with_lock(|io| {
            let tctl = io.read_smn(THM_TCON_CUR_TMP).ok().map(tctl_from_raw);
            let ccd_values: Vec<Option<f64>> = ccds
                .iter()
                .map(|ccd| {
                    io.read_smn(ccd_register(model, *ccd))
                        .ok()
                        .and_then(ccd_temp_from_raw)
                })
                .collect();
            let energy = energy_unit.and_then(|_| {
                io.read_msr(MSR_PKG_ENERGY_STAT)
                    .map_err(|message| tracing::debug!(%message, "amd package energy read failed"))
                    .ok()
            });
            (tctl, ccd_values, energy)
        });
        let Ok((tctl, ccd_values, energy)) = read else {
            return values;
        };
        if let (Some(unit), Some(raw)) = (energy_unit, energy) {
            let sample = Counter::new(package_energy_counter(raw), (self.clock)());
            let watts = self
                .previous_energy
                .and_then(|previous| per_second(previous, sample, Some(ENERGY_COUNTER_MODULUS)))
                .map(|ticks_per_second| ticks_per_second * unit)
                .and_then(plausible_watts);
            self.previous_energy = Some(sample);
            values.insert(id("power"), watts);
        }
        values.insert(id("tctl"), tctl);
        if offset < 0.0 {
            values.insert(id("tdie"), tctl.map(|t| t + offset));
        }
        for (ccd, value) in ccds.iter().zip(ccd_values) {
            values.insert(ccd_id(*ccd), value);
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
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeCpuIo {
        registers: HashMap<u32, u32>,
        msrs: HashMap<u32, VecDeque<u64>>,
        calls: Arc<Mutex<Vec<String>>>,
        locked: bool,
        refuse_lock: bool,
    }

    impl CpuIo for FakeCpuIo {
        fn read_smn(&mut self, address: u32) -> Result<u32, String> {
            assert!(self.locked, "smn read outside the pci lock");
            self.calls
                .lock()
                .unwrap()
                .push(format!("smn 0x{address:08x}"));
            self.registers
                .get(&address)
                .copied()
                .ok_or_else(|| "nak".to_string())
        }
        fn read_msr(&mut self, msr: u32) -> Result<u64, String> {
            assert!(self.locked, "msr read outside the pci lock");
            self.calls.lock().unwrap().push(format!("msr 0x{msr:08x}"));
            let queue = self.msrs.get_mut(&msr).ok_or_else(|| "nak".to_string())?;
            queue.pop_front().ok_or_else(|| "nak".to_string())
        }

        fn lock(&mut self, _timeout: Duration) -> Result<bool, String> {
            if self.refuse_lock {
                return Ok(false);
            }
            self.locked = true;
            self.calls.lock().unwrap().push("lock".to_string());
            Ok(true)
        }
        fn unlock(&mut self) {
            self.locked = false;
            self.calls.lock().unwrap().push("unlock".to_string());
        }
    }

    fn zen3() -> (FakeCpuIo, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registers = HashMap::new();
        registers.insert(THM_TCON_CUR_TMP, 400 << 21);
        registers.insert(ccd_register(0x21, 0), 2840);
        for ccd in 1..MAX_CCDS {
            registers.insert(ccd_register(0x21, ccd), 0);
        }
        (
            FakeCpuIo {
                registers,
                calls: calls.clone(),
                ..Default::default()
            },
            calls,
        )
    }

    #[test]
    fn enumerate_lists_tctl_and_only_populated_ccds_under_the_lock() {
        let (io, calls) = zen3();
        let mut backend =
            AmdCpuBackend::new(Box::new(io), 0x21, "AMD Ryzen 7 5800XT 8-Core Processor");
        let inventory = backend.enumerate().unwrap();
        assert_eq!(
            inventory
                .sensors
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["cpu/amd/tctl", "cpu/amd/ccd1"]
        );
        assert_eq!(inventory.sensors[0].label, "CPU Tctl/Tdie");
        assert!(inventory.controls.is_empty());
        let calls = calls.lock().unwrap();
        assert_eq!(calls.first().unwrap(), "lock");
        assert_eq!(calls.last().unwrap(), "unlock");
        assert_eq!(calls[1], "smn 0x00059800");
    }

    #[test]
    fn read_all_reports_tctl_and_ccd_values() {
        let (io, _) = zen3();
        let mut backend = AmdCpuBackend::new(Box::new(io), 0x21, "5800XT");
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["cpu/amd/tctl"], Some(50.0));
        assert_eq!(values["cpu/amd/ccd1"], Some(50.0));
    }

    #[test]
    fn offset_skus_expose_tdie_as_tctl_plus_offset() {
        let (io, _) = zen3();
        let mut backend =
            AmdCpuBackend::new(Box::new(io), 0x01, "AMD Ryzen 7 1800X Eight-Core Processor");
        let inventory = backend.enumerate().unwrap();
        assert_eq!(
            inventory
                .sensors
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["cpu/amd/tctl", "cpu/amd/tdie"]
        );
        let values = backend.read_all();
        assert_eq!(values["cpu/amd/tctl"], Some(50.0));
        assert_eq!(values["cpu/amd/tdie"], Some(30.0));
    }

    #[test]
    fn a_lock_timeout_yields_none_for_every_sensor_not_a_panic() {
        let (mut io, _) = zen3();
        let mut backend = AmdCpuBackend::new(Box::new(std::mem::take(&mut io)), 0x21, "x");
        backend.enumerate().unwrap();
        let (mut refusing, _) = zen3();
        refusing.refuse_lock = true;
        backend.io = Box::new(refusing);
        let values = backend.read_all();
        assert_eq!(values["cpu/amd/tctl"], None);
        assert_eq!(values["cpu/amd/ccd1"], None);
    }

    #[test]
    fn enumerate_fails_when_the_thermal_register_cannot_be_read() {
        let io = FakeCpuIo {
            calls: Arc::new(Mutex::new(Vec::new())),
            ..Default::default()
        };
        let mut backend = AmdCpuBackend::new(Box::new(io), 0x21, "x");
        assert!(matches!(backend.enumerate(), Err(HwError::Io { .. })));
    }
    fn zen3_with_energy(accumulator: [u64; 3]) -> FakeCpuIo {
        zen3_with_energy_unit(0x000A_1003, accumulator)
    }

    fn zen3_with_energy_unit(unit: u64, accumulator: [u64; 3]) -> FakeCpuIo {
        let (mut io, _) = zen3();
        io.msrs.insert(MSR_RAPL_PWR_UNIT, [unit].into());
        io.msrs.insert(MSR_PKG_ENERGY_STAT, accumulator.into());
        io
    }

    fn one_second_per_call() -> Box<dyn FnMut() -> Instant + Send> {
        let base = Instant::now();
        let mut seconds = 0;
        Box::new(move || {
            seconds += 1;
            base + Duration::from_secs(seconds)
        })
    }

    fn backend_with_energy(accumulator: [u64; 3]) -> AmdCpuBackend {
        AmdCpuBackend::new(Box::new(zen3_with_energy(accumulator)), 0x21, "5800XT")
            .with_clock(one_second_per_call())
    }

    #[test]
    fn package_power_joins_the_inventory_when_the_energy_msrs_read() {
        let mut backend = AmdCpuBackend::new(
            Box::new(zen3_with_energy([0, 100_000, 200_000])),
            0x21,
            "5800XT",
        );
        let inventory = backend.enumerate().unwrap();
        assert_eq!(
            inventory
                .sensors
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            vec!["cpu/amd/tctl", "cpu/amd/ccd1", "cpu/amd/power"]
        );
        let power = inventory.sensors.last().unwrap();
        assert_eq!(power.label, "CPU Package Power");
        assert_eq!(power.kind, SensorKind::Power);
    }

    #[test]
    fn the_first_energy_sample_reports_none_and_the_next_converts_ticks_to_watts() {
        let mut backend = backend_with_energy([0, 100_000, 200_000]);
        backend.enumerate().unwrap();
        assert_eq!(backend.read_all()["cpu/amd/power"], None);
        assert_eq!(
            backend.read_all()["cpu/amd/power"],
            Some(100_000.0 / 65536.0)
        );
    }

    #[test]
    fn a_wrapped_32_bit_accumulator_counts_across_the_modulus() {
        let mut backend = backend_with_energy([0, 0xFFFF_FF00, 0x0000_0100]);
        backend.enumerate().unwrap();
        backend.read_all();
        assert_eq!(backend.read_all()["cpu/amd/power"], Some(512.0 / 65536.0));
    }

    #[test]
    fn an_implausible_wattage_after_a_suspend_is_discarded() {
        let mut backend = backend_with_energy([0, 0, 200_000_000]);
        backend.enumerate().unwrap();
        backend.read_all();
        assert_eq!(backend.read_all()["cpu/amd/power"], None);
    }

    #[test]
    fn a_bogus_energy_unit_keeps_the_power_sensor_out_of_the_inventory() {
        let mut backend = AmdCpuBackend::new(
            Box::new(zen3_with_energy_unit(0x000A_0003, [0, 100_000, 200_000])),
            0x21,
            "5800XT",
        );
        let inventory = backend.enumerate().unwrap();
        assert!(inventory.sensors.iter().all(|s| s.id != "cpu/amd/power"));
        assert!(!backend.read_all().contains_key("cpu/amd/power"));
    }

    #[test]
    fn a_cpu_whose_energy_msrs_nak_keeps_its_temperature_sensors_only() {
        let (io, _) = zen3();
        let mut backend = AmdCpuBackend::new(Box::new(io), 0x21, "5800XT");
        let inventory = backend.enumerate().unwrap();
        assert!(inventory.sensors.iter().all(|s| s.id != "cpu/amd/power"));
        assert!(!backend.read_all().contains_key("cpu/amd/power"));
    }
}
