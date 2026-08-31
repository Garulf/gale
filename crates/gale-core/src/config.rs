use crate::curve::mix::MixMode;
use crate::Id;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
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
    pub active_profile: String,
    pub profiles: HashMap<String, ProfileConfig>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(default)]
    pub curves: HashMap<Id, CurveConfig>,
    #[serde(default)]
    pub assignments: HashMap<Id, Id>,
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
    fn bad_toml_is_a_parse_error() {
        assert!(matches!(
            GaleConfig::from_toml("not toml ["),
            Err(ConfigError::Parse(_))
        ));
    }
}
