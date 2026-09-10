use crate::curve::mix::MixMode;
use crate::presets::CurvePreset;
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

/// Whether a duty-cycle percentage is a finite value in the valid 0-100 range.
pub(crate) fn is_valid_duty(value: f64) -> bool {
    value.is_finite() && (0.0..=100.0).contains(&value)
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
    pub mqtt: MqttConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub labels: BTreeMap<Id, String>,
    #[serde(default)]
    pub controls: BTreeMap<Id, ControlSettings>,
    #[serde(default)]
    pub presets: BTreeMap<String, CurvePreset>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_duty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_duty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_duty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duty: Option<f64>,
}

impl ControlSettings {
    pub fn is_empty(&self) -> bool {
        self.min_duty.is_none()
            && self.start_duty.is_none()
            && self.stop_duty.is_none()
            && self.max_duty.is_none()
    }

    pub fn shape(&self, requested: f64, previous: f64) -> f64 {
        if let Some(stop) = self.stop_duty {
            if requested < stop {
                return 0.0;
            }
        }
        if requested <= 0.0 {
            return 0.0;
        }
        let mut duty = requested;
        if let Some(start) = self.start_duty {
            if previous <= 0.0 {
                duty = duty.max(start);
            }
        }
        if let Some(min) = self.min_duty {
            duty = duty.max(min);
        }
        if let Some(max) = self.max_duty {
            duty = duty.min(max);
        }
        duty
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MqttConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_mqtt_host")]
    pub host: String,
    #[serde(default = "default_mqtt_port")]
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default = "default_mqtt_client_id")]
    pub client_id: String,
    #[serde(default = "default_discovery_prefix")]
    pub discovery_prefix: String,
    #[serde(default = "default_topic_prefix")]
    pub topic_prefix: String,
}

fn default_mqtt_host() -> String {
    "localhost".to_string()
}

fn default_mqtt_port() -> u16 {
    1883
}

fn default_mqtt_client_id() -> String {
    "gale".to_string()
}

fn default_discovery_prefix() -> String {
    "homeassistant".to_string()
}

fn default_topic_prefix() -> String {
    "gale".to_string()
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_mqtt_host(),
            port: default_mqtt_port(),
            username: None,
            password: None,
            client_id: default_mqtt_client_id(),
            discovery_prefix: default_discovery_prefix(),
            topic_prefix: default_topic_prefix(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub graph: BTreeMap<String, BTreeMap<String, [f64; 2]>>,
    #[serde(default)]
    pub hidden: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub compact: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub groups: BTreeMap<String, BTreeMap<String, GroupUiConfig>>,
    #[serde(default)]
    pub dashboard: DashboardUiConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DashboardUiConfig {
    #[serde(default)]
    pub hidden: Vec<Id>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupUiConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub position: [f64; 2],
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<GroupPort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<GroupPort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupPort {
    pub node: String,
    pub handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HardwareConfig {
    #[serde(default)]
    pub corsair: CorsairConfig,
    #[serde(default)]
    pub dimm: DimmConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimmConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for DimmConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn default_true() -> bool {
    true
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
    Sum {
        inputs: Vec<Id>,
    },
    Subtract {
        inputs: Vec<Id>,
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
            VirtualSensorConfig::Sum { .. } => "sum",
            VirtualSensorConfig::Subtract { .. } => "subtract",
            VirtualSensorConfig::Offset { .. } => "offset",
            VirtualSensorConfig::Delta { .. } => "delta",
            VirtualSensorConfig::Webhook { .. } => "webhook",
        }
    }

    pub fn inputs(&self) -> Vec<Id> {
        match self {
            VirtualSensorConfig::Max { inputs }
            | VirtualSensorConfig::Min { inputs }
            | VirtualSensorConfig::Mean { inputs, .. }
            | VirtualSensorConfig::Sum { inputs }
            | VirtualSensorConfig::Subtract { inputs } => inputs.clone(),
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
            | VirtualSensorConfig::Sum { .. }
            | VirtualSensorConfig::Subtract { .. }
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

        // Depth-first topological sort with an explicit stack rather than recursion:
        // a config with a long virtual-sensor dependency chain would otherwise risk
        // overflowing the call stack (validated for chains 2000 deep in tests below).
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
                CurveConfig::Linear { sensor, .. } => Some(sensor.clone()),
                CurveConfig::Trigger { sensor, .. } => Some(sensor.clone()),
                CurveConfig::Target { sensor, .. } => Some(sensor.clone()),
                CurveConfig::Flat { .. }
                | CurveConfig::Mix { .. }
                | CurveConfig::Sync { .. }
                | CurveConfig::Offset { .. } => None,
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
            | CurveConfig::Linear { sensor, .. }
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
            CurveConfig::Sync { source } | CurveConfig::Offset { source, .. } => {
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
    Linear {
        sensor: Id,
        min_temp: f64,
        max_temp: f64,
        min_duty: f64,
        max_duty: f64,
        #[serde(default)]
        hysteresis: Option<HysteresisConfig>,
        #[serde(default)]
        response: Option<ResponseConfig>,
    },
    Mix {
        sources: Vec<Id>,
        mode: MixMode,
    },
    Sync {
        source: Id,
    },
    Offset {
        source: Id,
        add: f64,
        scale: f64,
    },
    Trigger {
        sensor: Id,
        on_temp: f64,
        off_temp: f64,
        on_duty: f64,
        off_duty: f64,
        #[serde(default)]
        response: Option<ResponseConfig>,
    },
    Target {
        sensor: Id,
        target_temp: f64,
        step_pct_per_sec: f64,
        min_duty: f64,
        max_duty: f64,
        #[serde(default)]
        deadband: Option<f64>,
        #[serde(default)]
        idle_temp: Option<f64>,
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
            mqtt: MqttConfig::default(),
            ui: UiConfig::default(),
            labels: BTreeMap::new(),
            controls: BTreeMap::new(),
            presets: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
