use crate::commander_core::CommanderCore;
use crate::commander_pro::CommanderPro;
use crate::corsair_psu::CorsairPsu;
use crate::hydro_platinum::HydroPlatinum;
use crate::transport::HidapiTransport;
use crate::CorsairDevice;
use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory, SensorInfo};
use std::collections::{HashMap, HashSet};

const SERIAL_SLUG_MAX_LEN: usize = 12;

pub const VID_CORSAIR: u16 = 0x1b1c;

const PID_COMMANDER_PRO: u16 = 0x0c10;
const PID_OBSIDIAN_1000D: u16 = 0x1d00;
const PID_COMMANDER_CORE: u16 = 0x0c1c;
const PID_COMMANDER_CORE_XT: u16 = 0x0c2a;
const PID_COMMANDER_ST: u16 = 0x0c32;

const COMMANDER_CORE_INTERFACE: i32 = 0;

// PIDs and fan counts from liquidctl's HydroPlatinum._MATCHES
// (liquidctl/driver/hydro_platinum.py)
const HYDRO_PLATINUM_MATCHES: &[(u16, &str, usize)] = &[
    (0x0c18, "h100i-platinum", 2),
    (0x0c19, "h100i-platinum-se", 2),
    (0x0c17, "h115i-platinum", 2),
    (0x0c29, "h60i-pro-xt", 2),
    (0x0c20, "h100i-pro-xt", 2),
    (0x0c21, "h115i-pro-xt", 2),
    (0x0c22, "h150i-pro-xt", 3),
    (0x0c35, "h100i-elite-rgb", 2),
    (0x0c36, "h115i-elite-rgb", 2),
    (0x0c37, "h150i-elite-rgb", 3),
    (0x0c40, "h100i-elite-rgb-white", 2),
    (0x0c41, "h150i-elite-rgb-white", 3),
];

// PIDs from liquidctl's CorsairHidPsu._MATCHES
// (liquidctl/driver/corsair_hid_psu.py)
const CORSAIR_PSU_MATCHES: &[(u16, &str)] = &[
    (0x1c05, "hx750i"),
    (0x1c06, "hx850i"),
    (0x1c07, "hx1000i"),
    (0x1c08, "hx1200i"),
    (0x1c23, "hx1200i-atx31"),
    (0x1c27, "hx1200i-atx31-2"),
    (0x1c0a, "rm650i"),
    (0x1c0b, "rm750i"),
    (0x1c0c, "rm850i"),
    (0x1c0d, "rm1000i"),
    (0x1c1e, "hx1000i-2022"),
    (0x1c1f, "hx1500i"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DriverKind {
    CommanderPro,
    CommanderCore { has_pump: bool },
    HydroPlatinum { fan_count: usize },
    CorsairPsu,
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
        _ => HYDRO_PLATINUM_MATCHES
            .iter()
            .find(|(match_pid, _, _)| *match_pid == pid)
            .map(|(_, slug, fan_count)| {
                (
                    DriverKind::HydroPlatinum {
                        fan_count: *fan_count,
                    },
                    *slug,
                )
            })
            .or_else(|| {
                CORSAIR_PSU_MATCHES
                    .iter()
                    .find(|(match_pid, _)| *match_pid == pid)
                    .map(|(_, slug)| (DriverKind::CorsairPsu, *slug))
            }),
    }
}

fn build_driver(
    kind: DriverKind,
    slug: &'static str,
    transport: Box<HidapiTransport>,
) -> Box<dyn CorsairDevice> {
    match kind {
        DriverKind::CommanderPro => Box::new(CommanderPro::new(transport, slug)),
        DriverKind::CommanderCore { has_pump } => {
            Box::new(CommanderCore::new(transport, slug, has_pump))
        }
        DriverKind::HydroPlatinum { fan_count } => {
            Box::new(HydroPlatinum::new(transport, slug, fan_count))
        }
        DriverKind::CorsairPsu => Box::new(CorsairPsu::new(transport, slug)),
    }
}

fn sanitize_serial(serial: &str) -> String {
    serial
        .chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .take(SERIAL_SLUG_MAX_LEN)
        .collect()
}

fn assign_slug(family: &str, serial: Option<&str>, taken: &HashSet<String>) -> String {
    if let Some(serial) = serial {
        let sanitized = sanitize_serial(serial);
        if !sanitized.is_empty() {
            let candidate = format!("{family}-{sanitized}");
            if !taken.contains(&candidate) {
                return candidate;
            }
        }
    }
    if !taken.contains(family) {
        return family.to_string();
    }
    let mut suffix = 1;
    loop {
        let candidate = format!("{family}-{suffix}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

pub struct CorsairBackend {
    slug: String,
    device: Box<dyn CorsairDevice>,
    controls: HashMap<Id, String>,
}

impl CorsairBackend {
    pub fn with_device(slug: String, device: Box<dyn CorsairDevice>) -> Self {
        Self {
            slug,
            device,
            controls: HashMap::new(),
        }
    }

    pub fn open_all() -> Vec<CorsairBackend> {
        let mut opened: Vec<(String, Option<String>, Box<dyn CorsairDevice>)> = Vec::new();
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
                let serial = info.serial_number().map(str::to_string);
                match api.open_path(info.path()) {
                    Ok(device) => {
                        let transport = Box::new(HidapiTransport::new(device));
                        let driver = build_driver(kind, slug, transport);
                        opened.push((slug.to_string(), serial, driver));
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
        let mut taken: HashSet<String> = HashSet::new();
        opened
            .into_iter()
            .map(|(family, serial, device)| {
                let slug = assign_slug(&family, serial.as_deref(), &taken);
                taken.insert(slug.clone());
                CorsairBackend::with_device(slug, device)
            })
            .collect()
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
        self.controls.clear();
        let channels = self
            .device
            .channels()
            .map_err(|message| device_error(&self.slug, message))?;
        let mut inventory = Inventory::default();
        for (channel, kind, label) in channels.sensors {
            let id = format!("corsair/{}/{channel}", self.slug);
            inventory.sensors.push(SensorInfo { id, label, kind });
        }
        for (channel, label) in channels.controls {
            let id = format!("corsair/{}/{channel}", self.slug);
            self.controls.insert(id.clone(), channel);
            inventory.controls.push(ControlInfo { id, label });
        }
        inventory.sensors.sort_by(|a, b| a.id.cmp(&b.id));
        inventory.controls.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(inventory)
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        self.device
            .read()
            .into_iter()
            .map(|(channel, value)| (format!("corsair/{}/{channel}", self.slug), value))
            .collect()
    }

    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
        let channel = self
            .controls
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        if !pct.is_finite() {
            return Err(non_finite_error(id));
        }
        self.device
            .set_duty(channel, pct)
            .map_err(|message| device_error(id, message))
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        let channel = self
            .controls
            .get(id)
            .ok_or_else(|| HwError::UnknownId(id.to_string()))?;
        self.device
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
        fail_channels: bool,
    }

    impl FakeDevice {
        fn new(slug: &str) -> Self {
            Self {
                slug: slug.to_string(),
                temp: Some(30.0),
                fan: Some(1000.0),
                duty: 50.0,
                released: false,
                fail_channels: false,
            }
        }

        fn failing_channels(slug: &str) -> Self {
            Self {
                fail_channels: true,
                ..Self::new(slug)
            }
        }
    }

    impl CorsairDevice for FakeDevice {
        fn slug(&self) -> &str {
            &self.slug
        }

        fn channels(&mut self) -> Result<DeviceChannels, String> {
            if self.fail_channels {
                return Err("boom".to_string());
            }
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
    fn hydro_platinum_family_pids_map_to_the_hydro_platinum_driver() {
        assert_eq!(
            driver_for(VID_CORSAIR, 0x0c17),
            Some((DriverKind::HydroPlatinum { fan_count: 2 }, "h115i-platinum"))
        );
        assert_eq!(
            driver_for(VID_CORSAIR, 0x0c22),
            Some((DriverKind::HydroPlatinum { fan_count: 3 }, "h150i-pro-xt"))
        );
        assert_eq!(
            driver_for(VID_CORSAIR, 0x0c41),
            Some((
                DriverKind::HydroPlatinum { fan_count: 3 },
                "h150i-elite-rgb-white"
            ))
        );
        assert_eq!(driver_for(VID_CORSAIR, 0x0c99), None);
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
    fn hxi_and_rmi_psu_pids_map_to_the_corsair_psu_driver() {
        assert_eq!(
            driver_for(VID_CORSAIR, 0x1c07),
            Some((DriverKind::CorsairPsu, "hx1000i"))
        );
        assert_eq!(
            driver_for(VID_CORSAIR, 0x1c0a),
            Some((DriverKind::CorsairPsu, "rm650i"))
        );
        assert_eq!(
            driver_for(VID_CORSAIR, 0x1c1f),
            Some((DriverKind::CorsairPsu, "hx1500i"))
        );
        assert_eq!(driver_for(VID_CORSAIR, 0x1cff), None);
    }

    #[test]
    fn enumerate_shapes_ids_as_corsair_slug_channel() {
        let mut backend =
            CorsairBackend::with_device("h100i".into(), Box::new(FakeDevice::new("h100i")));
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
    fn assign_slug_uses_family_and_sanitized_serial() {
        let taken = std::collections::HashSet::new();
        assert_eq!(assign_slug("h100i", Some("ABC123"), &taken), "h100i-abc123");
    }

    #[test]
    fn assign_slug_falls_back_to_family_when_serial_is_missing_or_empty() {
        let taken = std::collections::HashSet::new();
        assert_eq!(assign_slug("h100i", Some(""), &taken), "h100i");
        assert_eq!(assign_slug("h100i", None, &taken), "h100i");
    }

    #[test]
    fn assign_slug_falls_back_to_dash_n_on_collision() {
        let mut taken = std::collections::HashSet::new();
        taken.insert("h100i-abc123".to_string());
        assert_eq!(assign_slug("h100i", Some("ABC123"), &taken), "h100i");

        taken.insert("h100i".to_string());
        assert_eq!(assign_slug("h100i", Some("ABC123"), &taken), "h100i-1");
    }

    #[test]
    fn assign_slug_sanitizes_case_and_non_alphanumeric_characters() {
        let taken = std::collections::HashSet::new();
        assert_eq!(
            assign_slug("h100i", Some("AB-12_34!!"), &taken),
            "h100i-ab1234"
        );
    }

    #[test]
    fn assign_slug_truncates_sanitized_serial_to_twelve_characters() {
        let taken = std::collections::HashSet::new();
        assert_eq!(
            assign_slug("h100i", Some("ABCDEFGHIJKLMNOP"), &taken),
            "h100i-abcdefghijkl"
        );
    }

    #[test]
    fn a_failing_device_backend_errors_without_affecting_a_sibling_backend() {
        let mut failing = CorsairBackend::with_device(
            "h100i".into(),
            Box::new(FakeDevice::failing_channels("h100i")),
        );
        let mut healthy =
            CorsairBackend::with_device("h150i".into(), Box::new(FakeDevice::new("h150i")));

        assert!(failing.enumerate().is_err());

        let inventory = healthy.enumerate().unwrap();
        let sensor_ids: Vec<_> = inventory.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            sensor_ids,
            vec!["corsair/h150i/fan1", "corsair/h150i/temp1"]
        );
        healthy.set_duty("corsair/h150i/pwm1", 75.0).unwrap();
        let values = healthy.read_all();
        assert_eq!(values["corsair/h150i/pwm1"], Some(75.0));
    }

    #[test]
    fn read_all_merges_values_from_the_device() {
        let mut backend =
            CorsairBackend::with_device("h100i".into(), Box::new(FakeDevice::new("h100i")));
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
        let mut backend = CorsairBackend::with_device("h100i".into(), Box::new(device));
        backend.enumerate().unwrap();
        let values = backend.read_all();
        assert_eq!(values["corsair/h100i/temp1"], None);
        assert_eq!(values["corsair/h100i/fan1"], Some(1000.0));
    }

    #[test]
    fn set_duty_routes_to_the_device() {
        let mut backend =
            CorsairBackend::with_device("h150i".into(), Box::new(FakeDevice::new("h150i")));
        backend.enumerate().unwrap();
        backend.set_duty("corsair/h150i/pwm1", 75.0).unwrap();
        let values = backend.read_all();
        assert_eq!(values["corsair/h150i/pwm1"], Some(75.0));
    }

    #[test]
    fn set_duty_rejects_non_finite_percentages() {
        let mut backend =
            CorsairBackend::with_device("h100i".into(), Box::new(FakeDevice::new("h100i")));
        backend.enumerate().unwrap();
        let result = backend.set_duty("corsair/h100i/pwm1", f64::NAN);
        assert!(matches!(result, Err(HwError::Io { .. })));
    }

    #[test]
    fn set_duty_and_release_reject_unknown_ids() {
        let mut backend =
            CorsairBackend::with_device("h100i".into(), Box::new(FakeDevice::new("h100i")));
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
    fn release_routes_to_the_device() {
        let mut backend =
            CorsairBackend::with_device("h100i".into(), Box::new(FakeDevice::new("h100i")));
        backend.enumerate().unwrap();
        backend.release("corsair/h100i/pwm1").unwrap();
    }
}
