use crate::commander_core::CommanderCore;
use crate::commander_pro::CommanderPro;
use crate::transport::HidapiTransport;
use crate::CorsairDevice;
use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo};
use std::collections::HashMap;

pub const VID_CORSAIR: u16 = 0x1b1c;

const PID_COMMANDER_PRO: u16 = 0x0c10;
const PID_OBSIDIAN_1000D: u16 = 0x1d00;
const PID_COMMANDER_CORE: u16 = 0x0c1c;
const PID_COMMANDER_CORE_XT: u16 = 0x0c2a;
const PID_COMMANDER_ST: u16 = 0x0c32;

const COMMANDER_CORE_INTERFACE: i32 = 0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DriverKind {
    CommanderPro,
    CommanderCore { has_pump: bool },
}

fn driver_for(vid: u16, pid: u16) -> Option<(DriverKind, &'static str)> {
    if vid != VID_CORSAIR {
        return None;
    }
    match pid {
        PID_COMMANDER_PRO => Some((DriverKind::CommanderPro, "commander-pro")),
        PID_OBSIDIAN_1000D => Some((DriverKind::CommanderPro, "obsidian-1000d")),
        PID_COMMANDER_CORE => Some((
            DriverKind::CommanderCore { has_pump: true },
            "commander-core",
        )),
        PID_COMMANDER_CORE_XT => Some((
            DriverKind::CommanderCore { has_pump: false },
            "commander-core-xt",
        )),
        PID_COMMANDER_ST => Some((DriverKind::CommanderCore { has_pump: true }, "commander-st")),
        _ => None,
    }
}

pub struct CorsairBackend {
    devices: Vec<Box<dyn CorsairDevice>>,
    assigned_slugs: Vec<String>,
    controls_index: HashMap<Id, (usize, String)>,
}

impl CorsairBackend {
    pub fn new() -> Self {
        let mut devices: Vec<Box<dyn CorsairDevice>> = Vec::new();
        if let Ok(api) = hidapi::HidApi::new() {
            for info in api.device_list() {
                let vid = info.vendor_id();
                let pid = info.product_id();
                let Some((kind, slug)) = driver_for(vid, pid) else {
                    continue;
                };
                if matches!(kind, DriverKind::CommanderCore { .. })
                    && info.interface_number() != COMMANDER_CORE_INTERFACE
                {
                    continue;
                }
                match api.open_path(info.path()) {
                    Ok(device) => {
                        let transport = Box::new(HidapiTransport::new(device));
                        let driver: Box<dyn CorsairDevice> = match kind {
                            DriverKind::CommanderPro => {
                                Box::new(CommanderPro::new(transport, slug))
                            }
                            DriverKind::CommanderCore { has_pump } => {
                                Box::new(CommanderCore::new(transport, slug, has_pump))
                            }
                        };
                        devices.push(driver);
                    }
                    Err(error) => {
                        tracing::warn!(
                            vid,
                            pid,
                            %error,
                            "matched known corsair device but failed to open it"
                        );
                    }
                }
            }
        }
        Self::with_devices(devices)
    }

    pub fn with_devices(devices: Vec<Box<dyn CorsairDevice>>) -> Self {
        Self {
            devices,
            assigned_slugs: Vec::new(),
            controls_index: HashMap::new(),
        }
    }

    fn assign_slugs(&self) -> Vec<String> {
        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut slugs = Vec::with_capacity(self.devices.len());
        for device in &self.devices {
            let base = device.slug().to_string();
            let seen = counts.entry(base.clone()).or_insert(0);
            let slug = if *seen == 0 {
                base.clone()
            } else {
                format!("{base}-{seen}")
            };
            *seen += 1;
            slugs.push(slug);
        }
        slugs
    }
}

impl Default for CorsairBackend {
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

impl Backend for CorsairBackend {
    fn name(&self) -> &str {
        "corsair"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        let slugs = self.assign_slugs();
        self.assigned_slugs = slugs.clone();
        self.controls_index.clear();
        let mut inventory = Inventory::default();
        for (index, (slug, device)) in slugs.iter().zip(self.devices.iter_mut()).enumerate() {
            let channels = device
                .channels()
                .map_err(|message| device_error(slug, message))?;
            for (channel, kind, label) in channels.sensors {
                let id = format!("corsair/{slug}/{channel}");
                inventory.sensors.push(SensorInfo { id, label, kind });
            }
            for (channel, label) in channels.controls {
                let id = format!("corsair/{slug}/{channel}");
                self.controls_index.insert(id.clone(), (index, channel));
                inventory.controls.push(ControlInfo { id, label });
            }
        }
        inventory.sensors.sort_by(|a, b| a.id.cmp(&b.id));
        inventory.controls.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(inventory)
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        let mut values = HashMap::new();
        for (slug, device) in self.assigned_slugs.iter().zip(self.devices.iter_mut()) {
            for (channel, value) in device.read() {
                let id = format!("corsair/{slug}/{channel}");
                values.insert(id, value);
            }
        }
        values
    }

    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
        let (index, channel) = self
            .controls_index
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        if !pct.is_finite() {
            return Err(non_finite_error(id));
        }
        self.devices[*index]
            .set_duty(channel, pct)
            .map_err(|message| device_error(id, message))
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        let (index, channel) = self
            .controls_index
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        self.devices[*index]
            .release(channel)
            .map_err(|message| device_error(id, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeviceChannels;
    use gale_hw::SensorKind;

    struct FakeDevice {
        slug: String,
        temp: Option<f64>,
        fan: Option<f64>,
        duty: f64,
        released: bool,
    }

    impl FakeDevice {
        fn new(slug: &str) -> Self {
            Self {
                slug: slug.to_string(),
                temp: Some(30.0),
                fan: Some(1000.0),
                duty: 50.0,
                released: false,
            }
        }
    }

    impl CorsairDevice for FakeDevice {
        fn slug(&self) -> &str {
            &self.slug
        }

        fn channels(&mut self) -> Result<DeviceChannels, String> {
            Ok(DeviceChannels {
                sensors: vec![
                    ("temp1".into(), SensorKind::Temp, "Coolant".into()),
                    ("fan1".into(), SensorKind::Rpm, "Pump".into()),
                ],
                controls: vec![("pwm1".into(), "Pump PWM".into())],
            })
        }

        fn read(&mut self) -> HashMap<String, Option<f64>> {
            [
                ("temp1".to_string(), self.temp),
                ("fan1".to_string(), self.fan),
                ("pwm1".to_string(), Some(self.duty)),
            ]
            .into()
        }

        fn set_duty(&mut self, channel: &str, pct: f64) -> Result<(), String> {
            if channel != "pwm1" {
                return Err(format!("unknown channel {channel}"));
            }
            self.duty = pct;
            Ok(())
        }

        fn release(&mut self, channel: &str) -> Result<(), String> {
            if channel != "pwm1" {
                return Err(format!("unknown channel {channel}"));
            }
            self.released = true;
            Ok(())
        }
    }

    #[test]
    fn commander_core_family_pids_map_to_the_core_driver() {
        assert_eq!(
            driver_for(VID_CORSAIR, 0x0c1c),
            Some((
                DriverKind::CommanderCore { has_pump: true },
                "commander-core"
            ))
        );
        assert_eq!(
            driver_for(VID_CORSAIR, 0x0c2a),
            Some((
                DriverKind::CommanderCore { has_pump: false },
                "commander-core-xt"
            ))
        );
        assert_eq!(
            driver_for(VID_CORSAIR, 0x0c32),
            Some((DriverKind::CommanderCore { has_pump: true }, "commander-st"))
        );
        assert_eq!(
            driver_for(VID_CORSAIR, 0x0c10),
            Some((DriverKind::CommanderPro, "commander-pro"))
        );
        assert_eq!(driver_for(VID_CORSAIR, 0x0c33), None);
        assert_eq!(driver_for(0x1234, 0x0c1c), None);
    }

    #[test]
    fn enumerate_shapes_ids_as_corsair_slug_channel() {
        let mut backend = CorsairBackend::with_devices(vec![Box::new(FakeDevice::new("h100i"))]);
        let inventory = backend.enumerate().unwrap();
        let sensor_ids: Vec<_> = inventory.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            sensor_ids,
            vec!["corsair/h100i/fan1", "corsair/h100i/temp1"]
        );
        let control_ids: Vec<_> = inventory.controls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(control_ids, vec!["corsair/h100i/pwm1"]);
    }

    #[test]
    fn duplicate_slugs_get_suffixes_in_enumeration_order() {
        let mut backend = CorsairBackend::with_devices(vec![
            Box::new(FakeDevice::new("h100i")),
            Box::new(FakeDevice::new("h100i")),
        ]);
        let inventory = backend.enumerate().unwrap();
        let control_ids: Vec<_> = inventory.controls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            control_ids,
            vec!["corsair/h100i-1/pwm1", "corsair/h100i/pwm1"]
        );
    }

    #[test]
    fn scan_matching_zero_devices_yields_empty_inventory() {
        let mut backend = CorsairBackend::with_devices(vec![]);
        let inventory = backend.enumerate().unwrap();
        assert!(inventory.sensors.is_empty());
        assert!(inventory.controls.is_empty());
    }

    #[test]
    fn read_all_merges_values_from_every_device() {
        let mut backend = CorsairBackend::with_devices(vec![Box::new(FakeDevice::new("h100i"))]);
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["corsair/h100i/temp1"], Some(30.0));
        assert_eq!(values["corsair/h100i/fan1"], Some(1000.0));
        assert_eq!(values["corsair/h100i/pwm1"], Some(50.0));
    }

    #[test]
    fn unreadable_channel_reads_none_not_zero() {
        let mut device = FakeDevice::new("h100i");
        device.temp = None;
        let mut backend = CorsairBackend::with_devices(vec![Box::new(device)]);
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["corsair/h100i/temp1"], None);
        assert_eq!(values["corsair/h100i/fan1"], Some(1000.0));
    }

    #[test]
    fn set_duty_routes_to_owning_device() {
        let mut backend = CorsairBackend::with_devices(vec![
            Box::new(FakeDevice::new("h100i")),
            Box::new(FakeDevice::new("h150i")),
        ]);
        backend.enumerate().unwrap();
        backend.set_duty("corsair/h150i/pwm1", 75.0).unwrap();
        let values = backend.read_all();
        assert_eq!(values["corsair/h150i/pwm1"], Some(75.0));
        assert_eq!(values["corsair/h100i/pwm1"], Some(50.0));
    }

    #[test]
    fn set_duty_rejects_non_finite_percentages() {
        let mut backend = CorsairBackend::with_devices(vec![Box::new(FakeDevice::new("h100i"))]);
        backend.enumerate().unwrap();
        let result = backend.set_duty("corsair/h100i/pwm1", f64::NAN);
        assert!(matches!(result, Err(HwError::Io { .. })));
    }

    #[test]
    fn set_duty_and_release_reject_unknown_ids() {
        let mut backend = CorsairBackend::with_devices(vec![Box::new(FakeDevice::new("h100i"))]);
        backend.enumerate().unwrap();
        assert!(matches!(
            backend.set_duty("corsair/ghost/pwm9", 10.0),
            Err(HwError::UnknownId(_))
        ));
        assert!(matches!(
            backend.release("corsair/ghost/pwm9"),
            Err(HwError::UnknownId(_))
        ));
    }

    #[test]
    fn release_routes_to_owning_device() {
        let mut backend = CorsairBackend::with_devices(vec![Box::new(FakeDevice::new("h100i"))]);
        backend.enumerate().unwrap();
        backend.release("corsair/h100i/pwm1").unwrap();
    }
}
