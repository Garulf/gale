use crate::facade::{NvmlFacade, RealNvml};
use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo, SensorKind};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
struct FanControl {
    gpu: u32,
    fan: u32,
}

enum SensorTarget {
    Temp(u32),
    Fan(u32, u32),
}

pub struct NvidiaBackend {
    facade: Option<Box<dyn NvmlFacade>>,
    sensors: HashMap<Id, SensorTarget>,
    controls: HashMap<Id, FanControl>,
    claimed: HashSet<Id>,
}

impl NvidiaBackend {
    pub fn new() -> Self {
        let facade = RealNvml::try_init().map(|nvml| Box::new(nvml) as Box<dyn NvmlFacade>);
        Self::from_facade(facade)
    }

    pub fn with_facade(facade: Box<dyn NvmlFacade>) -> Self {
        Self::from_facade(Some(facade))
    }

    fn from_facade(facade: Option<Box<dyn NvmlFacade>>) -> Self {
        Self {
            facade,
            sensors: HashMap::new(),
            controls: HashMap::new(),
            claimed: HashSet::new(),
        }
    }
}

impl Default for NvidiaBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn non_finite_error(id: &str) -> HwError {
    HwError::Io {
        path: id.to_string(),
        message: "non-finite duty".into(),
    }
}

fn device_error(id: &str, message: String) -> HwError {
    HwError::Io {
        path: id.to_string(),
        message,
    }
}

impl Backend for NvidiaBackend {
    fn name(&self) -> &str {
        "nvidia"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        self.sensors.clear();
        self.controls.clear();
        self.claimed.clear();
        let mut inventory = Inventory::default();
        let Some(facade) = &self.facade else {
            return Ok(inventory);
        };
        let count = match facade.device_count() {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, "nvidia device_count failed");
                return Ok(inventory);
            }
        };
        for gpu in 0..count {
            let device = match facade.device(gpu) {
                Ok(device) => device,
                Err(error) => {
                    tracing::warn!(gpu, %error, "nvidia device open failed, skipping");
                    continue;
                }
            };
            let temp_id = format!("nvidia/{gpu}/temp");
            self.sensors
                .insert(temp_id.clone(), SensorTarget::Temp(gpu));
            inventory.sensors.push(SensorInfo {
                id: temp_id,
                label: format!("GPU {gpu} Temp"),
                kind: SensorKind::Temp,
            });
            for fan in 0..device.fan_count() {
                let fan_id = format!("nvidia/{gpu}/fan{fan}");
                self.sensors
                    .insert(fan_id.clone(), SensorTarget::Fan(gpu, fan));
                inventory.sensors.push(SensorInfo {
                    id: fan_id.clone(),
                    label: format!("GPU {gpu} Fan {fan}"),
                    kind: SensorKind::Duty,
                });
                if device.fan_controllable(fan) {
                    self.controls
                        .insert(fan_id.clone(), FanControl { gpu, fan });
                    inventory.controls.push(ControlInfo {
                        id: fan_id,
                        label: format!("GPU {gpu} Fan {fan}"),
                    });
                }
            }
        }
        inventory.sensors.sort_by(|a, b| a.id.cmp(&b.id));
        inventory.controls.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(inventory)
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let mut values = HashMap::new();
        let Some(facade) = &self.facade else {
            return values;
        };
        for (id, target) in &self.sensors {
            let value = match target {
                SensorTarget::Temp(gpu) => facade.device(*gpu).ok().and_then(|d| d.temperature()),
                SensorTarget::Fan(gpu, fan) => {
                    facade.device(*gpu).ok().and_then(|d| d.fan_duty(*fan))
                }
            };
            values.insert(id.clone(), value);
        }
        values
    }

    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
        let control = *self
            .controls
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        if !pct.is_finite() {
            return Err(non_finite_error(id));
        }
        let clamped = pct.clamp(0.0, 100.0).round();
        let facade = self
            .facade
            .as_ref()
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        let mut device = facade
            .device(control.gpu)
            .map_err(|error| device_error(id, error))?;
        device
            .set_fan_duty(control.fan, clamped)
            .map_err(|error| device_error(id, error))?;
        self.claimed.insert(id.to_string());
        Ok(())
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        let control = *self
            .controls
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        if !self.claimed.remove(id) {
            return Ok(());
        }
        let facade = self
            .facade
            .as_ref()
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        let mut device = facade
            .device(control.gpu)
            .map_err(|error| device_error(id, error))?;
        device
            .restore_fan_default(control.fan)
            .map_err(|error| device_error(id, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facade::NvmlDevice;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeState {
        temp: Option<f64>,
        duty: HashMap<u32, Option<f64>>,
        rpm: HashMap<u32, Option<f64>>,
        controllable: HashMap<u32, bool>,
        restored: Vec<u32>,
        fan_count: u32,
        fail_device_count: bool,
        fail_device: HashSet<u32>,
    }

    struct FakeFacade {
        state: Arc<Mutex<FakeState>>,
        gpus: u32,
    }

    struct FakeDevice {
        state: Arc<Mutex<FakeState>>,
    }

    impl NvmlFacade for FakeFacade {
        fn device_count(&self) -> Result<u32, String> {
            if self.state.lock().unwrap().fail_device_count {
                return Err("nvml down".into());
            }
            Ok(self.gpus)
        }

        fn device(&self, index: u32) -> Result<Box<dyn NvmlDevice>, String> {
            if self.state.lock().unwrap().fail_device.contains(&index) {
                return Err(format!("gpu {index} unavailable"));
            }
            Ok(Box::new(FakeDevice {
                state: self.state.clone(),
            }))
        }
    }

    impl NvmlDevice for FakeDevice {
        fn name(&self) -> String {
            "fake-gpu".into()
        }

        fn temperature(&self) -> Option<f64> {
            self.state.lock().unwrap().temp
        }

        fn fan_count(&self) -> u32 {
            self.state.lock().unwrap().fan_count
        }

        fn fan_duty(&self, fan: u32) -> Option<f64> {
            self.state.lock().unwrap().duty.get(&fan).copied().flatten()
        }

        fn fan_rpm(&self, fan: u32) -> Option<f64> {
            self.state.lock().unwrap().rpm.get(&fan).copied().flatten()
        }

        fn set_fan_duty(&mut self, fan: u32, pct: f64) -> Result<(), String> {
            self.state.lock().unwrap().duty.insert(fan, Some(pct));
            Ok(())
        }

        fn restore_fan_default(&mut self, fan: u32) -> Result<(), String> {
            self.state.lock().unwrap().restored.push(fan);
            Ok(())
        }

        fn fan_controllable(&self, fan: u32) -> bool {
            self.state
                .lock()
                .unwrap()
                .controllable
                .get(&fan)
                .copied()
                .unwrap_or(true)
        }
    }

    fn single_gpu(fan_count: u32) -> (NvidiaBackend, Arc<Mutex<FakeState>>) {
        let state = Arc::new(Mutex::new(FakeState {
            fan_count,
            temp: Some(55.0),
            ..Default::default()
        }));
        for fan in 0..fan_count {
            state.lock().unwrap().duty.insert(fan, Some(40.0));
            state.lock().unwrap().controllable.insert(fan, true);
        }
        let backend = NvidiaBackend::with_facade(Box::new(FakeFacade {
            state: state.clone(),
            gpus: 1,
        }));
        (backend, state)
    }

    #[test]
    fn empty_backend_with_no_facade_contributes_no_devices() {
        let mut backend = NvidiaBackend::from_facade(None);
        let inventory = backend.enumerate().unwrap();
        assert!(inventory.sensors.is_empty());
        assert!(inventory.controls.is_empty());
        assert!(backend.read_all().is_empty());
    }

    #[test]
    fn enumerate_shapes_temp_and_fan_ids_per_gpu() {
        let (mut backend, _state) = single_gpu(2);
        let inventory = backend.enumerate().unwrap();
        let sensor_ids: Vec<_> = inventory.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            sensor_ids,
            vec!["nvidia/0/fan0", "nvidia/0/fan1", "nvidia/0/temp"]
        );
        let control_ids: Vec<_> = inventory.controls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(control_ids, vec!["nvidia/0/fan0", "nvidia/0/fan1"]);
    }

    #[test]
    fn fan_sensor_kind_is_duty_even_when_uncontrollable() {
        let (mut backend, state) = single_gpu(1);
        state.lock().unwrap().controllable.insert(0, false);
        let inventory = backend.enumerate().unwrap();
        let fan = inventory
            .sensors
            .iter()
            .find(|s| s.id == "nvidia/0/fan0")
            .unwrap();
        assert_eq!(fan.kind, SensorKind::Duty);
        assert!(inventory.controls.is_empty());
    }

    #[test]
    fn read_all_reports_temp_and_duty_values() {
        let (mut backend, _state) = single_gpu(1);
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["nvidia/0/temp"], Some(55.0));
        assert_eq!(values["nvidia/0/fan0"], Some(40.0));
    }

    #[test]
    fn unreadable_sensor_reads_none_not_zero() {
        let (mut backend, state) = single_gpu(1);
        state.lock().unwrap().temp = None;
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["nvidia/0/temp"], None);
        assert_eq!(values["nvidia/0/fan0"], Some(40.0));
    }

    #[test]
    fn set_duty_clamps_and_rounds_before_writing() {
        let (mut backend, state) = single_gpu(1);
        backend.enumerate().unwrap();
        backend.set_duty("nvidia/0/fan0", 150.0).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(100.0));
        backend.set_duty("nvidia/0/fan0", -5.0).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(0.0));
        backend.set_duty("nvidia/0/fan0", 33.6).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(34.0));
    }

    #[test]
    fn set_duty_rejects_non_finite_before_calling_the_device() {
        let (mut backend, state) = single_gpu(1);
        backend.enumerate().unwrap();
        let result = backend.set_duty("nvidia/0/fan0", f64::NAN);
        assert!(matches!(result, Err(HwError::Io { .. })));
        assert_eq!(state.lock().unwrap().duty[&0], Some(40.0));
    }

    #[test]
    fn set_duty_and_release_reject_unknown_ids() {
        let (mut backend, _state) = single_gpu(1);
        backend.enumerate().unwrap();
        assert!(matches!(
            backend.set_duty("nvidia/9/fan9", 10.0),
            Err(HwError::UnknownId(_))
        ));
        assert!(matches!(
            backend.release("nvidia/9/fan9"),
            Err(HwError::UnknownId(_))
        ));
    }

    #[test]
    fn release_restores_default_only_when_previously_claimed() {
        let (mut backend, state) = single_gpu(1);
        backend.enumerate().unwrap();
        backend.release("nvidia/0/fan0").unwrap();
        assert!(state.lock().unwrap().restored.is_empty());

        backend.set_duty("nvidia/0/fan0", 60.0).unwrap();
        backend.release("nvidia/0/fan0").unwrap();
        assert_eq!(state.lock().unwrap().restored, vec![0]);

        backend.release("nvidia/0/fan0").unwrap();
        assert_eq!(state.lock().unwrap().restored, vec![0]);
    }

    #[test]
    fn uncontrollable_fans_reject_set_duty() {
        let (mut backend, state) = single_gpu(1);
        state.lock().unwrap().controllable.insert(0, false);
        backend.enumerate().unwrap();
        assert!(matches!(
            backend.set_duty("nvidia/0/fan0", 50.0),
            Err(HwError::UnknownId(_))
        ));
    }

    #[test]
    fn a_gpu_that_fails_to_open_is_skipped_without_erroring_the_backend() {
        let state = Arc::new(Mutex::new(FakeState {
            fan_count: 1,
            temp: Some(50.0),
            ..Default::default()
        }));
        state.lock().unwrap().duty.insert(0, Some(30.0));
        state.lock().unwrap().controllable.insert(0, true);
        state.lock().unwrap().fail_device.insert(0);
        let mut backend = NvidiaBackend::with_facade(Box::new(FakeFacade { state, gpus: 1 }));
        let inventory = backend.enumerate().unwrap();
        assert!(inventory.sensors.is_empty());
        assert!(inventory.controls.is_empty());
    }

    #[test]
    fn device_count_failure_yields_an_empty_inventory_without_erroring() {
        let state = Arc::new(Mutex::new(FakeState {
            fail_device_count: true,
            ..Default::default()
        }));
        let mut backend = NvidiaBackend::with_facade(Box::new(FakeFacade { state, gpus: 1 }));
        let inventory = backend.enumerate().unwrap();
        assert!(inventory.sensors.is_empty());
        assert!(inventory.controls.is_empty());
    }
}
