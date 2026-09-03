use crate::curve::mix::MixMode;
use crate::Id;
use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

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
    #[serde(default)]
    pub hardware: HardwareConfig,
    pub active_profile: String,
    pub profiles: BTreeMap<String, ProfileConfig>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(default)]
    pub curves: BTreeMap<Id, CurveConfig>,
    #[serde(default)]
    pub assignments: BTreeMap<Id, Id>,
}

impl ProfileConfig {
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
                },
            )]
            .into(),
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
            },
        );
        cfg.profiles.insert(
            "aaa".to_string(),
            ProfileConfig {
                curves: BTreeMap::new(),
                assignments: BTreeMap::new(),
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
}
