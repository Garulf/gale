use std::collections::HashMap;
use std::time::Duration;

use gale_hw::{Backend, HwError, Id, Inventory, SensorInfo, SensorKind};

use crate::decode::{
    brand_tctl_offset, ccd_register, ccd_temp_from_raw, supports_per_ccd, tctl_from_raw, MAX_CCDS,
    THM_TCON_CUR_TMP,
};

pub const PCI_LOCK_TIMEOUT: Duration = Duration::from_millis(10);
pub const PREFIX: &str = "cpu/amd";

pub trait SmnReader: Send {
    fn read_smn(&mut self, address: u32) -> Result<u32, String>;
    fn lock(&mut self, timeout: Duration) -> Result<bool, String>;
    fn unlock(&mut self);
}

pub struct AmdCpuBackend {
    io: Box<dyn SmnReader>,
    model: u32,
    tctl_offset: f64,
    ccds: Vec<u32>,
}

fn id(name: &str) -> Id {
    format!("{PREFIX}/{name}")
}

fn ccd_id(ccd: u32) -> Id {
    id(&format!("ccd{}", ccd + 1))
}

impl AmdCpuBackend {
    pub fn new(io: Box<dyn SmnReader>, model: u32, brand: &str) -> Self {
        Self {
            io,
            model,
            tctl_offset: brand_tctl_offset(brand),
            ccds: Vec::new(),
        }
    }

    fn with_lock<T>(&mut self, f: impl FnOnce(&mut dyn SmnReader) -> T) -> Result<T, String> {
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
                Ok::<_, String>((tctl, ccds))
            })
            .and_then(|r| r)
            .map_err(|message| HwError::Io {
                path: PREFIX.to_string(),
                message,
            })?;
        self.ccds = probe.1;
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
        Ok(Inventory {
            sensors,
            controls: Vec::new(),
        })
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let model = self.model;
        let ccds = self.ccds.clone();
        let offset = self.tctl_offset;
        let mut values: HashMap<Id, Option<f64>> = HashMap::new();
        values.insert(id("tctl"), None);
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
            (tctl, ccd_values)
        });
        let Ok((tctl, ccd_values)) = read else {
            return values;
        };
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
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeSmn {
        registers: HashMap<u32, u32>,
        calls: Arc<Mutex<Vec<String>>>,
        locked: bool,
        refuse_lock: bool,
    }

    impl SmnReader for FakeSmn {
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

    fn zen3() -> (FakeSmn, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registers = HashMap::new();
        registers.insert(THM_TCON_CUR_TMP, 400 << 21);
        registers.insert(ccd_register(0x21, 0), 2840);
        for ccd in 1..MAX_CCDS {
            registers.insert(ccd_register(0x21, ccd), 0);
        }
        (
            FakeSmn {
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
        let io = FakeSmn {
            calls: Arc::new(Mutex::new(Vec::new())),
            ..Default::default()
        };
        let mut backend = AmdCpuBackend::new(Box::new(io), 0x21, "x");
        assert!(matches!(backend.enumerate(), Err(HwError::Io { .. })));
    }
}
