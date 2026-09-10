use crate::facade::{NvmlDevice, NvmlFacade, RealNvml};
use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo, SensorKind};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
struct FanControl {
    gpu: u32,
    fan: u32,
    min_duty: f64,
    max_duty: f64,
}

enum SensorTarget {
    Temp(u32),
    Fan(u32, u32),
    Metric(u32, Metric),
}

#[derive(Clone, Copy)]
enum Metric {
    Util,
    MemUtil,
    Vram,
    VramPct,
    Clock,
    MemClock,
    Power,
    PState,
}

const METRICS: [(Metric, &str, &str, SensorKind); 8] = [
    (Metric::Util, "util", "Usage", SensorKind::Percent),
    (
        Metric::MemUtil,
        "mem_util",
        "Memory Usage",
        SensorKind::Percent,
    ),
    (Metric::Vram, "vram", "VRAM", SensorKind::Memory),
    (Metric::VramPct, "vram_pct", "VRAM %", SensorKind::Percent),
    (Metric::Clock, "clock", "Clock", SensorKind::Clock),
    (
        Metric::MemClock,
        "mem_clock",
        "Memory Clock",
        SensorKind::Clock,
    ),
    (Metric::Power, "power", "Power", SensorKind::Power),
    (Metric::PState, "pstate", "Power State", SensorKind::State),
];

fn read_metric(device: &dyn NvmlDevice, metric: Metric) -> Option<f64> {
    match metric {
        Metric::Util => device.utilization().map(|(gpu, _)| gpu),
        Metric::MemUtil => device.utilization().map(|(_, mem)| mem),
        Metric::Vram => device.memory().map(|(used, _)| used),
        Metric::VramPct => device
            .memory()
            .and_then(|(used, total)| (total > 0.0).then(|| used / total * 100.0)),
        Metric::Clock => device.clocks().map(|(g, _)| g),
        Metric::MemClock => device.clocks().map(|(_, m)| m),
        Metric::Power => device.power_watts(),
        Metric::PState => device.performance_state(),
    }
}

pub struct NvidiaBackend {
    facade: Option<Box<dyn NvmlFacade>>,
    sensors: HashMap<Id, SensorTarget>,
    controls: HashMap<Id, FanControl>,
    claimed: HashSet<Id>,
    driver_controlled: HashSet<Id>,
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
            driver_controlled: HashSet::new(),
        }
    }
}

impl Default for NvidiaBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn enumerate_temp(sensors: &mut HashMap<Id, SensorTarget>, gpu: u32, inventory: &mut Inventory) {
    let temp_id = format!("nvidia/{gpu}/temp");
    sensors.insert(temp_id.clone(), SensorTarget::Temp(gpu));
    inventory.sensors.push(SensorInfo {
        id: temp_id,
        label: format!("GPU {gpu} Temp"),
        kind: SensorKind::Temp,
    });
}

fn enumerate_fans(
    sensors: &mut HashMap<Id, SensorTarget>,
    controls: &mut HashMap<Id, FanControl>,
    gpu: u32,
    device: &dyn NvmlDevice,
    inventory: &mut Inventory,
) {
    for fan in 0..device.fan_count() {
        let fan_id = format!("nvidia/{gpu}/fan{fan}");
        sensors.insert(fan_id.clone(), SensorTarget::Fan(gpu, fan));
        inventory.sensors.push(SensorInfo {
            id: fan_id.clone(),
            label: format!("GPU {gpu} Fan {fan}"),
            kind: SensorKind::Duty,
        });
        if device.fan_controllable(fan) {
            let (min_duty, max_duty) = device.min_max_fan_duty(fan).unwrap_or((0.0, 100.0));
            controls.insert(
                fan_id.clone(),
                FanControl {
                    gpu,
                    fan,
                    min_duty,
                    max_duty,
                },
            );
            inventory.controls.push(ControlInfo {
                id: fan_id,
                label: format!("GPU {gpu} Fan {fan}"),
            });
        }
    }
}

fn enumerate_metrics(sensors: &mut HashMap<Id, SensorTarget>, gpu: u32, inventory: &mut Inventory) {
    for (metric, suffix, label, kind) in METRICS {
        let id = format!("nvidia/{gpu}/{suffix}");
        sensors.insert(id.clone(), SensorTarget::Metric(gpu, metric));
        inventory.sensors.push(SensorInfo {
            id,
            label: format!("GPU {gpu} {label}"),
            kind,
        });
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
        self.driver_controlled.clear();
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
            enumerate_temp(&mut self.sensors, gpu, &mut inventory);
            enumerate_fans(
                &mut self.sensors,
                &mut self.controls,
                gpu,
                device.as_ref(),
                &mut inventory,
            );
            enumerate_metrics(&mut self.sensors, gpu, &mut inventory);
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
                SensorTarget::Metric(gpu, metric) => facade
                    .device(*gpu)
                    .ok()
                    .and_then(|d| read_metric(d.as_ref(), *metric)),
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
        let facade = self
            .facade
            .as_ref()
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        let mut device = facade
            .device(control.gpu)
            .map_err(|error| device_error(id, error))?;
        if pct <= 0.0 {
            if !self.driver_controlled.contains(id) {
                device
                    .restore_fan_default(control.fan)
                    .map_err(|error| device_error(id, error))?;
                self.driver_controlled.insert(id.to_string());
            }
            self.claimed.insert(id.to_string());
            return Ok(());
        }
        let clamped = pct
            .clamp(0.0, 100.0)
            .clamp(control.min_duty, control.max_duty)
            .round();
        device
            .set_fan_duty(control.fan, clamped)
            .map_err(|error| device_error(id, error))?;
        self.driver_controlled.remove(id);
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
        if self.driver_controlled.remove(id) {
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
        fan_range: HashMap<u32, (f64, f64)>,
        restored: Vec<u32>,
        fan_count: u32,
        fail_device_count: bool,
        fail_device: HashSet<u32>,
        util: Option<(f64, f64)>,
        memory: Option<(f64, f64)>,
        clocks: Option<(f64, f64)>,
        power: Option<f64>,
        pstate: Option<f64>,
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
                .unwrap_or(false)
        }

        fn min_max_fan_duty(&self, fan: u32) -> Option<(f64, f64)> {
            self.state.lock().unwrap().fan_range.get(&fan).copied()
        }

        fn utilization(&self) -> Option<(f64, f64)> {
            self.state.lock().unwrap().util
        }

        fn memory(&self) -> Option<(f64, f64)> {
            self.state.lock().unwrap().memory
        }

        fn clocks(&self) -> Option<(f64, f64)> {
            self.state.lock().unwrap().clocks
        }

        fn power_watts(&self) -> Option<f64> {
            self.state.lock().unwrap().power
        }

        fn performance_state(&self) -> Option<f64> {
            self.state.lock().unwrap().pstate
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
            vec![
                "nvidia/0/clock",
                "nvidia/0/fan0",
                "nvidia/0/fan1",
                "nvidia/0/mem_clock",
                "nvidia/0/mem_util",
                "nvidia/0/power",
                "nvidia/0/pstate",
                "nvidia/0/temp",
                "nvidia/0/util",
                "nvidia/0/vram",
                "nvidia/0/vram_pct",
            ]
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
    fn enumerate_exposes_the_gpu_metrics_with_their_kinds() {
        let (mut backend, _state) = single_gpu(1);
        let inventory = backend.enumerate().unwrap();
        let kinds: HashMap<_, _> = inventory
            .sensors
            .iter()
            .map(|s| (s.id.as_str(), s.kind))
            .collect();
        assert_eq!(kinds["nvidia/0/util"], SensorKind::Percent);
        assert_eq!(kinds["nvidia/0/mem_util"], SensorKind::Percent);
        assert_eq!(kinds["nvidia/0/vram"], SensorKind::Memory);
        assert_eq!(kinds["nvidia/0/vram_pct"], SensorKind::Percent);
        assert_eq!(kinds["nvidia/0/clock"], SensorKind::Clock);
        assert_eq!(kinds["nvidia/0/mem_clock"], SensorKind::Clock);
        assert_eq!(kinds["nvidia/0/power"], SensorKind::Power);
        assert_eq!(kinds["nvidia/0/pstate"], SensorKind::State);
        let labels: HashMap<_, _> = inventory
            .sensors
            .iter()
            .map(|s| (s.id.as_str(), s.label.as_str()))
            .collect();
        assert_eq!(labels["nvidia/0/power"], "GPU 0 Power");
        assert_eq!(labels["nvidia/0/pstate"], "GPU 0 Power State");
    }

    #[test]
    fn read_all_reports_the_gpu_metrics_and_none_when_a_reading_fails() {
        let (mut backend, state) = single_gpu(1);
        {
            let mut s = state.lock().unwrap();
            s.util = Some((37.0, 12.0));
            s.memory = Some((4096.0, 16384.0));
            s.clocks = Some((2505.0, 10501.0));
            s.power = Some(312.5);
            s.pstate = Some(2.0);
        }
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["nvidia/0/util"], Some(37.0));
        assert_eq!(values["nvidia/0/mem_util"], Some(12.0));
        assert_eq!(values["nvidia/0/vram"], Some(4096.0));
        assert_eq!(values["nvidia/0/vram_pct"], Some(25.0));
        assert_eq!(values["nvidia/0/clock"], Some(2505.0));
        assert_eq!(values["nvidia/0/mem_clock"], Some(10501.0));
        assert_eq!(values["nvidia/0/power"], Some(312.5));
        assert_eq!(values["nvidia/0/pstate"], Some(2.0));

        state.lock().unwrap().power = None;
        state.lock().unwrap().memory = None;
        let values = backend.read_all();
        assert_eq!(values["nvidia/0/power"], None);
        assert_eq!(values["nvidia/0/vram_pct"], None);
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
        backend.set_duty("nvidia/0/fan0", 33.6).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(34.0));
    }

    #[test]
    fn zero_duty_hands_the_fan_back_to_the_driver_instead_of_writing() {
        let (mut backend, state) = single_gpu(1);
        state.lock().unwrap().fan_range.insert(0, (30.0, 100.0));
        backend.enumerate().unwrap();

        backend.set_duty("nvidia/0/fan0", 0.0).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(40.0));
        assert_eq!(state.lock().unwrap().restored, vec![0]);

        backend.set_duty("nvidia/0/fan0", -5.0).unwrap();
        backend.set_duty("nvidia/0/fan0", 0.0).unwrap();
        assert_eq!(state.lock().unwrap().restored, vec![0]);
    }

    #[test]
    fn positive_duty_after_zero_reclaims_manual_control_and_zero_again_restores_again() {
        let (mut backend, state) = single_gpu(1);
        backend.enumerate().unwrap();

        backend.set_duty("nvidia/0/fan0", 0.0).unwrap();
        backend.set_duty("nvidia/0/fan0", 55.0).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(55.0));
        assert_eq!(state.lock().unwrap().restored, vec![0]);

        backend.set_duty("nvidia/0/fan0", 0.0).unwrap();
        assert_eq!(state.lock().unwrap().restored, vec![0, 0]);
    }

    #[test]
    fn release_after_zero_duty_does_not_restore_a_second_time() {
        let (mut backend, state) = single_gpu(1);
        backend.enumerate().unwrap();

        backend.set_duty("nvidia/0/fan0", 0.0).unwrap();
        backend.release("nvidia/0/fan0").unwrap();
        assert_eq!(state.lock().unwrap().restored, vec![0]);

        backend.set_duty("nvidia/0/fan0", 50.0).unwrap();
        backend.release("nvidia/0/fan0").unwrap();
        assert_eq!(state.lock().unwrap().restored, vec![0, 0]);
    }

    #[test]
    fn set_duty_clamps_into_the_device_reported_min_max_fan_duty() {
        let (mut backend, state) = single_gpu(1);
        state.lock().unwrap().fan_range.insert(0, (30.0, 90.0));
        backend.enumerate().unwrap();

        backend.set_duty("nvidia/0/fan0", 15.0).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(30.0));

        backend.set_duty("nvidia/0/fan0", 50.0).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(50.0));

        backend.set_duty("nvidia/0/fan0", 95.0).unwrap();
        assert_eq!(state.lock().unwrap().duty[&0], Some(90.0));
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
    fn a_fan_with_no_explicit_controllability_probe_result_is_conservatively_uncontrollable() {
        let (mut backend, state) = single_gpu(1);
        state.lock().unwrap().controllable.remove(&0);
        let inventory = backend.enumerate().unwrap();
        assert!(inventory.sensors.iter().any(|s| s.id == "nvidia/0/fan0"));
        assert!(inventory.controls.is_empty());
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
