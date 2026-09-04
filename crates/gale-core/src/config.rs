use crate::curve::mix::MixMode;
use crate::Id;
use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigError {
    #[error("config parse error: {0}")]
    Parse(String),
    #[error("config serialize error: {0}")]
    Serialize(String),
    #[error("invalid config: {0}")]
    Invalid(String),
}

fn default_tick() -> u64 {
    1000
}

fn default_bind() -> String {
    "127.0.0.1:5250".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaleConfig {
    #[serde(default = "default_tick")]
    pub tick_interval_ms: u64,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub hardware: HardwareConfig,
    pub active_profile: String,
    pub profiles: BTreeMap<String, ProfileConfig>,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub graph: BTreeMap<String, BTreeMap<String, [f64; 2]>>,
    #[serde(default)]
    pub hidden: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HardwareConfig {
    #[serde(default)]
    pub corsair: CorsairConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CorsairConfig {
    #[serde(default)]
    pub on_release: CorsairReleaseMode,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum CorsairReleaseMode {
    PinFull,
    #[default]
    KeepLast,
    Fixed {
        percent: u8,
    },
}

impl Serialize for CorsairReleaseMode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            CorsairReleaseMode::PinFull => serializer.serialize_str("pin_full"),
            CorsairReleaseMode::KeepLast => serializer.serialize_str("keep_last"),
            CorsairReleaseMode::Fixed { percent } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("fixed", percent)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for CorsairReleaseMode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Named(String),
            Fixed { fixed: u8 },
        }

        match Repr::deserialize(deserializer)? {
            Repr::Named(value) if value == "pin_full" => Ok(CorsairReleaseMode::PinFull),
            Repr::Named(value) if value == "keep_last" => Ok(CorsairReleaseMode::KeepLast),
            Repr::Named(value) => Err(D::Error::custom(format!(
                "unknown on_release value '{value}'"
            ))),
            Repr::Fixed { fixed } => Ok(CorsairReleaseMode::Fixed { percent: fixed }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            api_key: None,
        }
    }
}

pub const VIRTUAL_PREFIX: &str = "virtual/";

pub fn virtual_id(name: &str) -> Id {
    format!("{VIRTUAL_PREFIX}{name}")
}

pub fn virtual_name(id: &str) -> Option<&str> {
    id.strip_prefix(VIRTUAL_PREFIX)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VirtualSensorConfig {
    Max {
        inputs: Vec<Id>,
    },
    Min {
        inputs: Vec<Id>,
    },
    Mean {
        inputs: Vec<Id>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_s: Option<f64>,
    },
    Offset {
        input: Id,
        add: f64,
        scale: f64,
    },
    Delta {
        input: Id,
        window_s: f64,
    },
    Webhook {
        #[serde(default)]
        token: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_s: Option<f64>,
    },
}

impl VirtualSensorConfig {
    pub fn type_name(&self) -> &'static str {
        match self {
            VirtualSensorConfig::Max { .. } => "max",
            VirtualSensorConfig::Min { .. } => "min",
            VirtualSensorConfig::Mean { .. } => "mean",
            VirtualSensorConfig::Offset { .. } => "offset",
            VirtualSensorConfig::Delta { .. } => "delta",
            VirtualSensorConfig::Webhook { .. } => "webhook",
        }
    }

    pub fn inputs(&self) -> Vec<Id> {
        match self {
            VirtualSensorConfig::Max { inputs }
            | VirtualSensorConfig::Min { inputs }
            | VirtualSensorConfig::Mean { inputs, .. } => inputs.clone(),
            VirtualSensorConfig::Offset { input, .. }
            | VirtualSensorConfig::Delta { input, .. } => {
                vec![input.clone()]
            }
            VirtualSensorConfig::Webhook { .. } => Vec::new(),
        }
    }
}

fn validate_virtual_sensor(
    name: &str,
    cfg: &VirtualSensorConfig,
    sensors: &BTreeMap<String, VirtualSensorConfig>,
) -> Result<(), ConfigError> {
    if name.is_empty() {
        return Err(ConfigError::Invalid(
            "virtual sensor name must not be empty".to_string(),
        ));
    }
    if name.contains('/') {
        return Err(ConfigError::Invalid(format!(
            "virtual sensor name '{name}' must not contain '/'"
        )));
    }

    let inputs = cfg.inputs();
    if matches!(
        cfg,
        VirtualSensorConfig::Max { .. }
            | VirtualSensorConfig::Min { .. }
            | VirtualSensorConfig::Mean { .. }
    ) && inputs.is_empty()
    {
        return Err(ConfigError::Invalid(format!(
            "virtual sensor '{name}' has no inputs"
        )));
    }

    for input in &inputs {
        if input.is_empty() {
            return Err(ConfigError::Invalid(format!(
                "virtual sensor '{name}' has an empty input id"
            )));
        }
    }

    for input in &inputs {
        if let Some(other) = virtual_name(input) {
            if !sensors.contains_key(other) {
                return Err(ConfigError::Invalid(format!(
                    "virtual sensor '{name}' references undefined virtual sensor '{input}'"
                )));
            }
        }
    }

    if let VirtualSensorConfig::Offset { add, scale, .. } = cfg {
        if !add.is_finite() {
            return Err(ConfigError::Invalid(format!(
                "virtual sensor '{name}' has a non-finite add"
            )));
        }
        if !scale.is_finite() {
            return Err(ConfigError::Invalid(format!(
                "virtual sensor '{name}' has a non-finite scale"
            )));
        }
    }

    if let VirtualSensorConfig::Webhook { token, timeout_s } = cfg {
        if token.is_empty() {
            return Err(ConfigError::Invalid(format!(
                "virtual sensor '{name}' has an empty token"
            )));
        }
        if let Some(timeout_s) = timeout_s {
            if !timeout_s.is_finite() || *timeout_s <= 0.0 {
                return Err(ConfigError::Invalid(format!(
                    "virtual sensor '{name}' timeout_s must be a positive number"
                )));
            }
        }
    }

    let window_s = match cfg {
        VirtualSensorConfig::Mean { window_s, .. } => *window_s,
        VirtualSensorConfig::Delta { window_s, .. } => Some(*window_s),
        _ => None,
    };
    if let Some(window_s) = window_s {
        if !window_s.is_finite() || window_s <= 0.0 {
            return Err(ConfigError::Invalid(format!(
                "virtual sensor '{name}' window_s must be a positive number"
            )));
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(default)]
    pub curves: BTreeMap<Id, CurveConfig>,
    #[serde(default)]
    pub assignments: BTreeMap<Id, Id>,
    #[serde(default)]
    pub sensors: BTreeMap<String, VirtualSensorConfig>,
}

impl ProfileConfig {
    pub fn hardware_sensors_used(&self) -> BTreeSet<Id> {
        let mut result = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut stack: Vec<Id> = self.assigned_sensors().into_iter().collect();
        while let Some(id) = stack.pop() {
            match virtual_name(&id) {
                Some(name) => {
                    if !visited.insert(name.to_string()) {
                        continue;
                    }
                    if let Some(cfg) = self.sensors.get(name) {
                        stack.extend(cfg.inputs());
                    }
                }
                None => {
                    result.insert(id);
                }
            }
        }
        result
    }

    pub fn validate_virtual_sensors(&self) -> Result<Vec<String>, ConfigError> {
        for (name, cfg) in &self.sensors {
            validate_virtual_sensor(name, cfg, &self.sensors)?;
        }

        let mut order = Vec::new();
        let mut visited = BTreeSet::new();
        for name in self.sensors.keys() {
            self.order_virtual_sensor(name, &mut visited, &mut order)?;
        }
        Ok(order)
    }

    fn virtual_dependencies(&self, name: &str) -> Vec<String> {
        self.sensors
            .get(name)
            .map(|cfg| {
                cfg.inputs()
                    .into_iter()
                    .filter_map(|input| virtual_name(&input).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn order_virtual_sensor(
        &self,
        name: &str,
        visited: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), ConfigError> {
        if visited.contains(name) {
            return Ok(());
        }

        struct Frame {
            name: String,
            children: Vec<String>,
            next: usize,
        }

        let mut stack = vec![Frame {
            name: name.to_string(),
            children: self.virtual_dependencies(name),
            next: 0,
        }];

        while !stack.is_empty() {
            let top = stack.len() - 1;
            if stack[top].next < stack[top].children.len() {
                let child = stack[top].children[stack[top].next].clone();
                stack[top].next += 1;

                if visited.contains(&child) {
                    continue;
                }
                if let Some(pos) = stack.iter().position(|frame| frame.name == child) {
                    let mut cycle: Vec<String> = stack[pos..]
                        .iter()
                        .map(|frame| frame.name.clone())
                        .collect();
                    cycle.push(child);
                    return Err(ConfigError::Invalid(format!(
                        "virtual sensor cycle: {}",
                        cycle.join(" -> ")
                    )));
                }

                let children = self.virtual_dependencies(&child);
                stack.push(Frame {
                    name: child,
                    children,
                    next: 0,
                });
            } else {
                let done = stack.pop().expect("stack is non-empty");
                visited.insert(done.name.clone());
                order.push(done.name);
            }
        }
        Ok(())
    }

    pub fn referenced_sensors(&self) -> BTreeSet<Id> {
        self.curves
            .values()
            .filter_map(|curve| match curve {
                CurveConfig::Point { sensor, .. } => Some(sensor.clone()),
                CurveConfig::Trigger { sensor, .. } => Some(sensor.clone()),
                CurveConfig::Target { sensor, .. } => Some(sensor.clone()),
                CurveConfig::Flat { .. } | CurveConfig::Mix { .. } | CurveConfig::Sync { .. } => {
                    None
                }
            })
            .collect()
    }

    pub fn assigned_sensors(&self) -> BTreeSet<Id> {
        let mut sensors = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for curve_name in self.assignments.values() {
            self.collect_curve_sensors(curve_name, &mut visited, &mut sensors);
        }
        sensors
    }

    fn collect_curve_sensors(
        &self,
        curve_name: &Id,
        visited: &mut BTreeSet<Id>,
        sensors: &mut BTreeSet<Id>,
    ) {
        if !visited.insert(curve_name.clone()) {
            return;
        }
        let Some(curve) = self.curves.get(curve_name) else {
            return;
        };
        match curve {
            CurveConfig::Point { sensor, .. }
            | CurveConfig::Trigger { sensor, .. }
            | CurveConfig::Target { sensor, .. } => {
                sensors.insert(sensor.clone());
            }
            CurveConfig::Flat { .. } => {}
            CurveConfig::Mix { sources, .. } => {
                for source in sources {
                    self.collect_curve_sensors(source, visited, sensors);
                }
            }
            CurveConfig::Sync { source } => {
                self.collect_curve_sensors(source, visited, sensors);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HysteresisConfig {
    pub up: f64,
    pub down: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseConfig {
    pub rise_pct_per_sec: f64,
    pub fall_pct_per_sec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveConfig {
    Point {
        sensor: Id,
        points: Vec<[f64; 2]>,
        #[serde(default)]
        hysteresis: Option<HysteresisConfig>,
        #[serde(default)]
        response: Option<ResponseConfig>,
    },
    Flat {
        duty: f64,
    },
    Mix {
        sources: Vec<Id>,
        mode: MixMode,
    },
    Sync {
        source: Id,
    },
    Trigger {
        sensor: Id,
        on_temp: f64,
        off_temp: f64,
        on_duty: f64,
        off_duty: f64,
    },
    Target {
        sensor: Id,
        target_temp: f64,
        step_pct_per_sec: f64,
        min_duty: f64,
        max_duty: f64,
    },
}

impl GaleConfig {
    pub fn from_toml(s: &str) -> Result<Self, ConfigError> {
        toml::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::Serialize(e.to_string()))
    }
}

impl GaleConfig {
    pub fn default_config() -> Self {
        Self {
            tick_interval_ms: default_tick(),
            api: ApiConfig::default(),
            hardware: HardwareConfig::default(),
            active_profile: "default".to_string(),
            profiles: [(
                "default".to_string(),
                ProfileConfig {
                    curves: BTreeMap::new(),
                    assignments: BTreeMap::new(),
                    sensors: BTreeMap::new(),
                },
            )]
            .into(),
            ui: UiConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
active_profile = "quiet"

[profiles.quiet.curves.cpu]
type = "point"
sensor = "hwmon/nct6798/temp1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.quiet.curves.cpu.hysteresis]
up = 2.0
down = 5.0

[profiles.quiet.curves.case]
type = "mix"
sources = ["cpu"]
mode = "max"

[profiles.quiet.assignments]
"hwmon/nct6798/pwm1" = "cpu"
"hwmon/nct6798/pwm2" = "case"
"#;

    #[test]
    fn parses_sample_config_with_defaults() {
        let cfg = GaleConfig::from_toml(SAMPLE).unwrap();
        assert_eq!(cfg.tick_interval_ms, 1000);
        assert_eq!(cfg.api.bind, "127.0.0.1:5250");
        assert_eq!(cfg.api.api_key, None);
        assert_eq!(cfg.active_profile, "quiet");
        let profile = &cfg.profiles["quiet"];
        assert_eq!(profile.assignments["hwmon/nct6798/pwm1"], "cpu");
        match &profile.curves["cpu"] {
            CurveConfig::Point {
                sensor,
                points,
                hysteresis,
                response,
            } => {
                assert_eq!(sensor, "hwmon/nct6798/temp1");
                assert_eq!(points, &vec![[30.0, 20.0], [70.0, 100.0]]);
                assert_eq!(hysteresis, &Some(HysteresisConfig { up: 2.0, down: 5.0 }));
                assert_eq!(response, &None);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let cfg = GaleConfig::from_toml(SAMPLE).unwrap();
        let rendered = cfg.to_toml().unwrap();
        assert_eq!(GaleConfig::from_toml(&rendered).unwrap(), cfg);
    }

    #[test]
    fn profiles_serialize_in_sorted_key_order_regardless_of_insertion_order() {
        let mut cfg = GaleConfig::default_config();
        cfg.profiles.insert(
            "zzz".to_string(),
            ProfileConfig {
                curves: BTreeMap::new(),
                assignments: BTreeMap::new(),
                sensors: BTreeMap::new(),
            },
        );
        cfg.profiles.insert(
            "aaa".to_string(),
            ProfileConfig {
                curves: BTreeMap::new(),
                assignments: BTreeMap::new(),
                sensors: BTreeMap::new(),
            },
        );
        let rendered = cfg.to_toml().unwrap();
        let aaa_pos = rendered.find("[profiles.aaa.curves]").unwrap();
        let default_pos = rendered.find("[profiles.default.curves]").unwrap();
        let zzz_pos = rendered.find("[profiles.zzz.curves]").unwrap();
        assert!(aaa_pos < default_pos);
        assert!(default_pos < zzz_pos);
    }

    #[test]
    fn hardware_defaults_to_corsair_keep_last_when_section_is_absent() {
        let cfg = GaleConfig::from_toml(SAMPLE).unwrap();
        assert_eq!(
            cfg.hardware.corsair.on_release,
            CorsairReleaseMode::KeepLast
        );
    }

    #[test]
    fn on_release_parses_pin_full() {
        let toml = format!("{SAMPLE}\n[hardware.corsair]\non_release = \"pin_full\"\n");
        let cfg = GaleConfig::from_toml(&toml).unwrap();
        assert_eq!(cfg.hardware.corsair.on_release, CorsairReleaseMode::PinFull);
    }

    #[test]
    fn on_release_parses_keep_last() {
        let toml = format!("{SAMPLE}\n[hardware.corsair]\non_release = \"keep_last\"\n");
        let cfg = GaleConfig::from_toml(&toml).unwrap();
        assert_eq!(
            cfg.hardware.corsair.on_release,
            CorsairReleaseMode::KeepLast
        );
    }

    #[test]
    fn on_release_parses_fixed_percent() {
        let toml = format!("{SAMPLE}\n[hardware.corsair]\non_release = {{ fixed = 50 }}\n");
        let cfg = GaleConfig::from_toml(&toml).unwrap();
        assert_eq!(
            cfg.hardware.corsair.on_release,
            CorsairReleaseMode::Fixed { percent: 50 }
        );
    }

    #[test]
    fn on_release_rejects_unknown_string() {
        let toml = format!("{SAMPLE}\n[hardware.corsair]\non_release = \"bogus\"\n");
        assert!(matches!(
            GaleConfig::from_toml(&toml),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn on_release_round_trips_through_toml_for_all_variants() {
        for mode in [
            CorsairReleaseMode::PinFull,
            CorsairReleaseMode::KeepLast,
            CorsairReleaseMode::Fixed { percent: 42 },
        ] {
            let mut cfg = GaleConfig::default_config();
            cfg.hardware.corsair.on_release = mode.clone();
            let rendered = cfg.to_toml().unwrap();
            let parsed = GaleConfig::from_toml(&rendered).unwrap();
            assert_eq!(parsed.hardware.corsair.on_release, mode);
        }
    }

    #[test]
    fn bad_toml_is_a_parse_error() {
        assert!(matches!(
            GaleConfig::from_toml("not toml ["),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn referenced_sensors_covers_all_variants() {
        let profile = ProfileConfig {
            curves: [
                (
                    "point".to_string(),
                    CurveConfig::Point {
                        sensor: "s_point".to_string(),
                        points: vec![[0.0, 0.0]],
                        hysteresis: None,
                        response: None,
                    },
                ),
                (
                    "trigger".to_string(),
                    CurveConfig::Trigger {
                        sensor: "s_trigger".to_string(),
                        on_temp: 60.0,
                        off_temp: 50.0,
                        on_duty: 100.0,
                        off_duty: 20.0,
                    },
                ),
                (
                    "target".to_string(),
                    CurveConfig::Target {
                        sensor: "s_target".to_string(),
                        target_temp: 60.0,
                        step_pct_per_sec: 5.0,
                        min_duty: 20.0,
                        max_duty: 100.0,
                    },
                ),
                ("flat".to_string(), CurveConfig::Flat { duty: 50.0 }),
                (
                    "mix".to_string(),
                    CurveConfig::Mix {
                        sources: vec!["point".to_string()],
                        mode: MixMode::Max,
                    },
                ),
                (
                    "sync".to_string(),
                    CurveConfig::Sync {
                        source: "point".to_string(),
                    },
                ),
            ]
            .into(),
            assignments: BTreeMap::new(),
            sensors: BTreeMap::new(),
        };
        let referenced = profile.referenced_sensors();
        assert_eq!(
            referenced,
            BTreeSet::from([
                "s_point".to_string(),
                "s_trigger".to_string(),
                "s_target".to_string(),
            ])
        );
    }

    #[test]
    fn assigned_sensors_empty_when_no_assignments() {
        let profile = ProfileConfig {
            curves: [(
                "cpu".to_string(),
                CurveConfig::Point {
                    sensor: "s_point".to_string(),
                    points: vec![[0.0, 0.0]],
                    hysteresis: None,
                    response: None,
                },
            )]
            .into(),
            assignments: BTreeMap::new(),
            sensors: BTreeMap::new(),
        };
        assert!(profile.assigned_sensors().is_empty());
    }

    #[test]
    fn assigned_sensors_returns_sensor_of_assigned_point_curve() {
        let profile = ProfileConfig {
            curves: [(
                "cpu".to_string(),
                CurveConfig::Point {
                    sensor: "s_point".to_string(),
                    points: vec![[0.0, 0.0]],
                    hysteresis: None,
                    response: None,
                },
            )]
            .into(),
            assignments: [("pwm1".to_string(), "cpu".to_string())].into(),
            sensors: BTreeMap::new(),
        };
        assert_eq!(
            profile.assigned_sensors(),
            BTreeSet::from(["s_point".to_string()])
        );
    }

    #[test]
    fn assigned_sensors_follows_mix_children() {
        let profile = ProfileConfig {
            curves: [
                (
                    "cpu".to_string(),
                    CurveConfig::Point {
                        sensor: "s_cpu".to_string(),
                        points: vec![[0.0, 0.0]],
                        hysteresis: None,
                        response: None,
                    },
                ),
                (
                    "gpu".to_string(),
                    CurveConfig::Point {
                        sensor: "s_gpu".to_string(),
                        points: vec![[0.0, 0.0]],
                        hysteresis: None,
                        response: None,
                    },
                ),
                (
                    "case".to_string(),
                    CurveConfig::Mix {
                        sources: vec!["cpu".to_string(), "gpu".to_string()],
                        mode: MixMode::Max,
                    },
                ),
            ]
            .into(),
            assignments: [("pwm1".to_string(), "case".to_string())].into(),
            sensors: BTreeMap::new(),
        };
        assert_eq!(
            profile.assigned_sensors(),
            BTreeSet::from(["s_cpu".to_string(), "s_gpu".to_string()])
        );
    }

    #[test]
    fn assigned_sensors_follows_sync_source() {
        let profile = ProfileConfig {
            curves: [
                (
                    "cpu".to_string(),
                    CurveConfig::Point {
                        sensor: "s_cpu".to_string(),
                        points: vec![[0.0, 0.0]],
                        hysteresis: None,
                        response: None,
                    },
                ),
                (
                    "case".to_string(),
                    CurveConfig::Sync {
                        source: "cpu".to_string(),
                    },
                ),
            ]
            .into(),
            assignments: [("pwm1".to_string(), "case".to_string())].into(),
            sensors: BTreeMap::new(),
        };
        assert_eq!(
            profile.assigned_sensors(),
            BTreeSet::from(["s_cpu".to_string()])
        );
    }

    #[test]
    fn assigned_sensors_tolerates_self_referencing_and_missing_mix_children() {
        let profile = ProfileConfig {
            curves: [(
                "loopy".to_string(),
                CurveConfig::Mix {
                    sources: vec!["loopy".to_string(), "ghost".to_string()],
                    mode: MixMode::Max,
                },
            )]
            .into(),
            assignments: [("pwm1".to_string(), "loopy".to_string())].into(),
            sensors: BTreeMap::new(),
        };
        assert!(profile.assigned_sensors().is_empty());
    }

    const VIRTUAL_SENSOR_SAMPLE: &str = r#"
active_profile = "default"

[profiles.default.sensors.cpu_hot]
type = "max"
inputs = ["superio/nct6798d/temp2", "nvidia/0/temp"]

[profiles.default.sensors.coolant_smooth]
type = "mean"
inputs = ["corsair/commander-pro-0805009c9327/temp1"]
window_s = 10

[profiles.default.sensors.coolant_rise]
type = "delta"
input = "virtual/coolant_smooth"
window_s = 30

[profiles.default.sensors.gpu_adjusted]
type = "offset"
input = "nvidia/0/temp"
add = -5.0
scale = 1.0
"#;

    #[test]
    fn virtual_sensor_toml_round_trips_every_node_type() {
        let cfg = GaleConfig::from_toml(VIRTUAL_SENSOR_SAMPLE).unwrap();
        let profile = &cfg.profiles["default"];
        assert_eq!(
            profile.sensors["cpu_hot"],
            VirtualSensorConfig::Max {
                inputs: vec![
                    "superio/nct6798d/temp2".to_string(),
                    "nvidia/0/temp".to_string(),
                ],
            }
        );
        match &profile.sensors["coolant_smooth"] {
            VirtualSensorConfig::Mean { window_s, .. } => {
                assert_eq!(*window_s, Some(10.0));
            }
            other => panic!("wrong variant: {other:?}"),
        }
        assert_eq!(
            profile.sensors["coolant_rise"],
            VirtualSensorConfig::Delta {
                input: "virtual/coolant_smooth".to_string(),
                window_s: 30.0,
            }
        );
        match &profile.sensors["gpu_adjusted"] {
            VirtualSensorConfig::Offset { add, scale, .. } => {
                assert_eq!(*add, -5.0);
                assert_eq!(*scale, 1.0);
            }
            other => panic!("wrong variant: {other:?}"),
        }

        let rendered = cfg.to_toml().unwrap();
        assert_eq!(GaleConfig::from_toml(&rendered).unwrap(), cfg);

        let mut extra_cfg = GaleConfig::default_config();
        extra_cfg
            .profiles
            .get_mut("default")
            .unwrap()
            .sensors
            .insert(
                "min_temp".to_string(),
                VirtualSensorConfig::Min {
                    inputs: vec!["hw/t1".to_string()],
                },
            );
        extra_cfg
            .profiles
            .get_mut("default")
            .unwrap()
            .sensors
            .insert(
                "windowless_mean".to_string(),
                VirtualSensorConfig::Mean {
                    inputs: vec!["hw/t2".to_string()],
                    window_s: None,
                },
            );
        let extra_rendered = extra_cfg.to_toml().unwrap();
        assert_eq!(GaleConfig::from_toml(&extra_rendered).unwrap(), extra_cfg);
        let windowless_section = extra_rendered
            .split("[profiles.default.sensors.windowless_mean]")
            .nth(1)
            .unwrap();
        let windowless_section = windowless_section.split("\n\n").next().unwrap();
        assert!(!windowless_section.contains("window_s"));
    }

    #[test]
    fn unknown_virtual_sensor_type_is_a_parse_error() {
        let toml = r#"
active_profile = "default"

[profiles.default.sensors.bogus]
type = "bogus"
inputs = ["hw/t1"]
"#;
        assert!(matches!(
            GaleConfig::from_toml(toml),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn config_without_sensors_table_parses_as_before() {
        let cfg = GaleConfig::from_toml(SAMPLE).unwrap();
        assert!(cfg.profiles["quiet"].sensors.is_empty());
    }

    #[test]
    fn virtual_id_and_name_round_trip() {
        assert_eq!(virtual_id("cpu_hot"), "virtual/cpu_hot");
        assert_eq!(virtual_name("virtual/cpu_hot"), Some("cpu_hot"));
        assert_eq!(virtual_name("hwmon/x/temp1"), None);
    }

    #[test]
    fn type_name_and_inputs_cover_every_variant() {
        let max = VirtualSensorConfig::Max {
            inputs: vec!["a".to_string(), "b".to_string()],
        };
        assert_eq!(max.type_name(), "max");
        assert_eq!(max.inputs(), vec!["a".to_string(), "b".to_string()]);

        let min = VirtualSensorConfig::Min {
            inputs: vec!["a".to_string()],
        };
        assert_eq!(min.type_name(), "min");
        assert_eq!(min.inputs(), vec!["a".to_string()]);

        let mean = VirtualSensorConfig::Mean {
            inputs: vec!["a".to_string()],
            window_s: None,
        };
        assert_eq!(mean.type_name(), "mean");
        assert_eq!(mean.inputs(), vec!["a".to_string()]);

        let offset = VirtualSensorConfig::Offset {
            input: "a".to_string(),
            add: 1.0,
            scale: 2.0,
        };
        assert_eq!(offset.type_name(), "offset");
        assert_eq!(offset.inputs(), vec!["a".to_string()]);

        let delta = VirtualSensorConfig::Delta {
            input: "a".to_string(),
            window_s: 5.0,
        };
        assert_eq!(delta.type_name(), "delta");
        assert_eq!(delta.inputs(), vec!["a".to_string()]);
    }

    #[test]
    fn validate_returns_dependency_order_for_spec_example() {
        let cfg = GaleConfig::from_toml(VIRTUAL_SENSOR_SAMPLE).unwrap();
        let profile = &cfg.profiles["default"];
        let order = profile.validate_virtual_sensors().unwrap();
        let smooth_pos = order.iter().position(|n| n == "coolant_smooth").unwrap();
        let rise_pos = order.iter().position(|n| n == "coolant_rise").unwrap();
        assert!(smooth_pos < rise_pos);
        assert_eq!(
            order,
            vec![
                "coolant_smooth".to_string(),
                "coolant_rise".to_string(),
                "cpu_hot".to_string(),
                "gpu_adjusted".to_string(),
            ]
        );
    }

    #[test]
    fn validate_orders_diamond_dependencies_before_their_shared_ancestor() {
        let profile = profile_with_sensors([
            (
                "a".to_string(),
                VirtualSensorConfig::Max {
                    inputs: vec!["virtual/b".to_string(), "virtual/c".to_string()],
                },
            ),
            (
                "b".to_string(),
                VirtualSensorConfig::Offset {
                    input: "hw/d".to_string(),
                    add: 1.0,
                    scale: 1.0,
                },
            ),
            (
                "c".to_string(),
                VirtualSensorConfig::Offset {
                    input: "hw/d".to_string(),
                    add: 2.0,
                    scale: 1.0,
                },
            ),
        ]);
        let order = profile.validate_virtual_sensors().unwrap();
        let a_pos = order.iter().position(|n| n == "a").unwrap();
        let b_pos = order.iter().position(|n| n == "b").unwrap();
        let c_pos = order.iter().position(|n| n == "c").unwrap();
        assert!(b_pos < a_pos);
        assert!(c_pos < a_pos);
    }

    #[test]
    fn validate_handles_a_very_long_chain_without_overflowing_the_stack() {
        const DEPTH: usize = 2000;
        let mut sensors: BTreeMap<String, VirtualSensorConfig> = BTreeMap::new();
        sensors.insert(
            "s0".to_string(),
            VirtualSensorConfig::Offset {
                input: "hw/t".to_string(),
                add: 0.0,
                scale: 1.0,
            },
        );
        for i in 1..DEPTH {
            sensors.insert(
                format!("s{i}"),
                VirtualSensorConfig::Offset {
                    input: format!("virtual/s{}", i - 1),
                    add: 0.0,
                    scale: 1.0,
                },
            );
        }
        let profile = profile_with_sensors(sensors);
        let order = profile.validate_virtual_sensors().unwrap();
        assert_eq!(order.len(), DEPTH);
    }

    fn profile_with_sensors(
        sensors: impl Into<BTreeMap<String, VirtualSensorConfig>>,
    ) -> ProfileConfig {
        ProfileConfig {
            curves: BTreeMap::new(),
            assignments: BTreeMap::new(),
            sensors: sensors.into(),
        }
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER_TOKEN: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    #[test]
    fn webhook_toml_round_trips_with_and_without_timeout() {
        let toml = format!(
            r#"
active_profile = "default"

[profiles.default.sensors.remote]
type = "webhook"
token = "{TOKEN}"
timeout_s = 60.0

[profiles.default.sensors.forever]
type = "webhook"
token = "{OTHER_TOKEN}"
"#
        );
        let cfg = GaleConfig::from_toml(&toml).unwrap();
        let profile = &cfg.profiles["default"];
        assert_eq!(
            profile.sensors["remote"],
            VirtualSensorConfig::Webhook {
                token: TOKEN.to_string(),
                timeout_s: Some(60.0),
            }
        );
        assert_eq!(
            profile.sensors["forever"],
            VirtualSensorConfig::Webhook {
                token: OTHER_TOKEN.to_string(),
                timeout_s: None,
            }
        );

        let rendered = cfg.to_toml().unwrap();
        assert_eq!(GaleConfig::from_toml(&rendered).unwrap(), cfg);

        let forever_section = rendered
            .split("[profiles.default.sensors.forever]")
            .nth(1)
            .unwrap();
        let forever_section = forever_section.split("\n\n").next().unwrap();
        assert!(!forever_section.contains("timeout_s"));
    }

    #[test]
    fn webhook_type_name_and_inputs() {
        let webhook = VirtualSensorConfig::Webhook {
            token: "t".to_string(),
            timeout_s: None,
        };
        assert_eq!(webhook.type_name(), "webhook");
        assert_eq!(webhook.inputs(), Vec::<Id>::new());
    }

    #[test]
    fn webhook_without_token_field_parses_with_empty_token() {
        let toml = r#"
active_profile = "default"

[profiles.default.sensors.w]
type = "webhook"
"#;
        let cfg = GaleConfig::from_toml(toml).unwrap();
        assert_eq!(
            cfg.profiles["default"].sensors["w"],
            VirtualSensorConfig::Webhook {
                token: String::new(),
                timeout_s: None,
            }
        );
    }

    #[test]
    fn rule_9_rejects_empty_webhook_token() {
        let profile = profile_with_sensors([(
            "x".to_string(),
            VirtualSensorConfig::Webhook {
                token: String::new(),
                timeout_s: None,
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' has an empty token".to_string()
            ))
        );
    }

    #[test]
    fn rule_10_rejects_invalid_timeout_s() {
        for timeout_s in [Some(0.0), Some(-1.0), Some(f64::NAN)] {
            let profile = profile_with_sensors([(
                "x".to_string(),
                VirtualSensorConfig::Webhook {
                    token: TOKEN.to_string(),
                    timeout_s,
                },
            )]);
            assert_eq!(
                profile.validate_virtual_sensors(),
                Err(ConfigError::Invalid(
                    "virtual sensor 'x' timeout_s must be a positive number".to_string()
                ))
            );
        }

        for timeout_s in [Some(60.0), None] {
            let profile = profile_with_sensors([(
                "x".to_string(),
                VirtualSensorConfig::Webhook {
                    token: TOKEN.to_string(),
                    timeout_s,
                },
            )]);
            assert_eq!(
                profile.validate_virtual_sensors(),
                Ok(vec!["x".to_string()])
            );
        }
    }

    #[test]
    fn webhook_is_a_valid_leaf_input_to_other_virtual_sensors() {
        let profile = profile_with_sensors([
            (
                "hook".to_string(),
                VirtualSensorConfig::Webhook {
                    token: TOKEN.to_string(),
                    timeout_s: None,
                },
            ),
            (
                "agg".to_string(),
                VirtualSensorConfig::Max {
                    inputs: vec!["virtual/hook".to_string()],
                },
            ),
        ]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Ok(vec!["hook".to_string(), "agg".to_string()])
        );
    }

    #[test]
    fn hardware_sensors_used_ignores_webhook_sensors() {
        let profile = ProfileConfig {
            curves: [(
                "c".to_string(),
                CurveConfig::Point {
                    sensor: "virtual/hook".to_string(),
                    points: vec![[0.0, 0.0]],
                    hysteresis: None,
                    response: None,
                },
            )]
            .into(),
            assignments: [("pwm1".to_string(), "c".to_string())].into(),
            sensors: [(
                "hook".to_string(),
                VirtualSensorConfig::Webhook {
                    token: TOKEN.to_string(),
                    timeout_s: None,
                },
            )]
            .into(),
        };
        assert!(profile.hardware_sensors_used().is_empty());
    }

    #[test]
    fn rule_1_rejects_empty_name() {
        let profile = profile_with_sensors([(
            String::new(),
            VirtualSensorConfig::Max {
                inputs: vec!["hw/t1".to_string()],
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor name must not be empty".to_string()
            ))
        );
    }

    #[test]
    fn rule_2_rejects_name_with_slash() {
        let profile = profile_with_sensors([(
            "a/b".to_string(),
            VirtualSensorConfig::Max {
                inputs: vec!["hw/t1".to_string()],
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor name 'a/b' must not contain '/'".to_string()
            ))
        );
    }

    #[test]
    fn rule_3_rejects_empty_inputs() {
        let profile =
            profile_with_sensors([("x".to_string(), VirtualSensorConfig::Max { inputs: vec![] })]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' has no inputs".to_string()
            ))
        );
    }

    #[test]
    fn rule_4_rejects_empty_input_id() {
        let profile = profile_with_sensors([(
            "x".to_string(),
            VirtualSensorConfig::Max {
                inputs: vec![String::new()],
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' has an empty input id".to_string()
            ))
        );
    }

    #[test]
    fn rule_5_rejects_reference_to_undefined_virtual_sensor() {
        let profile = profile_with_sensors([(
            "x".to_string(),
            VirtualSensorConfig::Max {
                inputs: vec!["virtual/other".to_string()],
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' references undefined virtual sensor 'virtual/other'"
                    .to_string()
            ))
        );
    }

    #[test]
    fn rule_6_rejects_non_finite_add_and_scale() {
        let profile = profile_with_sensors([(
            "x".to_string(),
            VirtualSensorConfig::Offset {
                input: "hw/t1".to_string(),
                add: f64::NAN,
                scale: 1.0,
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' has a non-finite add".to_string()
            ))
        );

        let profile = profile_with_sensors([(
            "x".to_string(),
            VirtualSensorConfig::Offset {
                input: "hw/t1".to_string(),
                add: 0.0,
                scale: f64::INFINITY,
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' has a non-finite scale".to_string()
            ))
        );
    }

    #[test]
    fn rule_7_rejects_invalid_window_s() {
        let profile = profile_with_sensors([(
            "x".to_string(),
            VirtualSensorConfig::Mean {
                inputs: vec!["hw/t1".to_string()],
                window_s: Some(0.0),
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' window_s must be a positive number".to_string()
            ))
        );

        let profile = profile_with_sensors([(
            "x".to_string(),
            VirtualSensorConfig::Delta {
                input: "hw/t1".to_string(),
                window_s: f64::NAN,
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor 'x' window_s must be a positive number".to_string()
            ))
        );
    }

    #[test]
    fn rule_8_rejects_cycle_between_two_sensors() {
        let profile = profile_with_sensors([
            (
                "a".to_string(),
                VirtualSensorConfig::Max {
                    inputs: vec!["virtual/b".to_string()],
                },
            ),
            (
                "b".to_string(),
                VirtualSensorConfig::Offset {
                    input: "virtual/a".to_string(),
                    add: 0.0,
                    scale: 1.0,
                },
            ),
        ]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor cycle: a -> b -> a".to_string()
            ))
        );
    }

    #[test]
    fn rule_8_rejects_self_reference() {
        let profile = profile_with_sensors([(
            "s".to_string(),
            VirtualSensorConfig::Min {
                inputs: vec!["virtual/s".to_string()],
            },
        )]);
        assert_eq!(
            profile.validate_virtual_sensors(),
            Err(ConfigError::Invalid(
                "virtual sensor cycle: s -> s".to_string()
            ))
        );
    }

    #[test]
    fn hardware_sensors_used_resolves_through_virtual_chain() {
        let profile = ProfileConfig {
            curves: [(
                "c".to_string(),
                CurveConfig::Point {
                    sensor: "virtual/a".to_string(),
                    points: vec![[0.0, 0.0]],
                    hysteresis: None,
                    response: None,
                },
            )]
            .into(),
            assignments: [("pwm1".to_string(), "c".to_string())].into(),
            sensors: [
                (
                    "a".to_string(),
                    VirtualSensorConfig::Max {
                        inputs: vec!["virtual/b".to_string(), "hw/t1".to_string()],
                    },
                ),
                (
                    "b".to_string(),
                    VirtualSensorConfig::Offset {
                        input: "hw/t2".to_string(),
                        add: 0.0,
                        scale: 1.0,
                    },
                ),
                (
                    "orphan".to_string(),
                    VirtualSensorConfig::Max {
                        inputs: vec!["hw/t9".to_string()],
                    },
                ),
            ]
            .into(),
        };
        assert_eq!(
            profile.hardware_sensors_used(),
            BTreeSet::from(["hw/t1".to_string(), "hw/t2".to_string()])
        );
    }

    #[test]
    fn hardware_sensors_used_drops_undefined_virtual_ids() {
        let profile = ProfileConfig {
            curves: [(
                "c".to_string(),
                CurveConfig::Point {
                    sensor: "virtual/ghost".to_string(),
                    points: vec![[0.0, 0.0]],
                    hysteresis: None,
                    response: None,
                },
            )]
            .into(),
            assignments: [("pwm1".to_string(), "c".to_string())].into(),
            sensors: BTreeMap::new(),
        };
        assert!(profile.hardware_sensors_used().is_empty());
    }

    #[test]
    fn hardware_sensors_used_matches_assigned_sensors_when_no_virtual_sensors() {
        let profile = ProfileConfig {
            curves: [
                (
                    "cpu".to_string(),
                    CurveConfig::Point {
                        sensor: "s_cpu".to_string(),
                        points: vec![[0.0, 0.0]],
                        hysteresis: None,
                        response: None,
                    },
                ),
                (
                    "gpu".to_string(),
                    CurveConfig::Point {
                        sensor: "s_gpu".to_string(),
                        points: vec![[0.0, 0.0]],
                        hysteresis: None,
                        response: None,
                    },
                ),
                (
                    "case".to_string(),
                    CurveConfig::Mix {
                        sources: vec!["cpu".to_string(), "gpu".to_string()],
                        mode: MixMode::Max,
                    },
                ),
            ]
            .into(),
            assignments: [("pwm1".to_string(), "case".to_string())].into(),
            sensors: BTreeMap::new(),
        };
        assert_eq!(profile.hardware_sensors_used(), profile.assigned_sensors());
        assert_eq!(
            profile.hardware_sensors_used(),
            BTreeSet::from(["s_cpu".to_string(), "s_gpu".to_string()])
        );
    }

    #[test]
    fn ui_table_round_trips() {
        let toml = r#"
active_profile = "default"

[profiles.default]

[ui.graph.default]
"sensor:hwmon/nct6798" = [40.0, 120.0]
"curve:cpu" = [580.0, 150.0]

[ui.hidden]
default = ["sensor:corsair/commander-pro-0805009c9327"]
"#;
        let cfg = GaleConfig::from_toml(toml).unwrap();
        let rendered = cfg.to_toml().unwrap();
        assert_eq!(GaleConfig::from_toml(&rendered).unwrap(), cfg);
        assert!(rendered.contains(r#""sensor:hwmon/nct6798""#));
        let round_tripped = GaleConfig::from_toml(&rendered).unwrap();
        assert_eq!(
            round_tripped.ui.graph["default"]["sensor:hwmon/nct6798"],
            [40.0, 120.0]
        );
    }

    #[test]
    fn config_without_ui_table_parses_with_empty_defaults() {
        let cfg = GaleConfig::from_toml(SAMPLE).unwrap();
        assert!(cfg.ui.graph.is_empty());
        assert!(cfg.ui.hidden.is_empty());
    }

    #[test]
    fn ui_positions_for_deleted_nodes_do_not_fail_validation() {
        let toml = r#"
active_profile = "default"

[profiles.default]

[ui.graph.default]
"curve:ghost" = [40.0, 120.0]

[profiles.default.assignments]
"pwm1" = "real"

[profiles.default.curves.real]
type = "flat"
duty = 50.0
"#;
        let cfg = GaleConfig::from_toml(toml).unwrap();
        assert_eq!(cfg.ui.graph["default"]["curve:ghost"], [40.0, 120.0]);
        crate::build::build_engine(&cfg).unwrap();
    }

    #[test]
    fn ui_table_is_ignored_by_build_engine() {
        let base = r#"
active_profile = "default"

[profiles.default.curves.real]
type = "flat"
duty = 50.0

[profiles.default.assignments]
"pwm1" = "real"
"#;
        let with_ui = format!(
            r#"{base}
[ui.graph.default]
"curve:real" = [40.0, 120.0]

[ui.hidden]
default = ["sensor:hwmon/nct6798"]
"#
        );

        let without = GaleConfig::from_toml(base).unwrap();
        let with = GaleConfig::from_toml(&with_ui).unwrap();

        let without_engine = crate::build::build_engine(&without).unwrap();
        let with_engine = crate::build::build_engine(&with).unwrap();

        assert_eq!(without_engine.assignments(), with_engine.assignments());
        assert_eq!(
            without_engine.virtual_sensor_ids(),
            with_engine.virtual_sensor_ids()
        );
    }
}
