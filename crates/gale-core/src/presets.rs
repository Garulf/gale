use crate::config::{ConfigError, GaleConfig, HysteresisConfig, ResponseConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurvePreset {
    Point {
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
        min_temp: f64,
        max_temp: f64,
        min_duty: f64,
        max_duty: f64,
        #[serde(default)]
        hysteresis: Option<HysteresisConfig>,
        #[serde(default)]
        response: Option<ResponseConfig>,
    },
    Trigger {
        on_temp: f64,
        off_temp: f64,
        on_duty: f64,
        off_duty: f64,
        #[serde(default)]
        response: Option<ResponseConfig>,
    },
    Target {
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

pub const BUILTIN_NAMES: [&str; 3] = ["Quiet", "Balanced", "Performance"];

fn point_preset(points: &[[f64; 2]]) -> CurvePreset {
    CurvePreset::Point {
        points: points.to_vec(),
        hysteresis: None,
        response: None,
    }
}

pub fn builtin() -> BTreeMap<String, CurvePreset> {
    BTreeMap::from([
        (
            "Quiet".to_string(),
            point_preset(&[
                [30.0, 20.0],
                [50.0, 30.0],
                [65.0, 50.0],
                [75.0, 80.0],
                [85.0, 100.0],
            ]),
        ),
        (
            "Balanced".to_string(),
            point_preset(&[[30.0, 30.0], [50.0, 45.0], [65.0, 70.0], [80.0, 100.0]]),
        ),
        (
            "Performance".to_string(),
            point_preset(&[[30.0, 45.0], [45.0, 65.0], [60.0, 90.0], [70.0, 100.0]]),
        ),
    ])
}

pub fn is_builtin_name(name: &str) -> bool {
    BUILTIN_NAMES
        .iter()
        .any(|builtin| builtin.eq_ignore_ascii_case(name))
}

fn duty_in_range(duty: f64) -> bool {
    duty.is_finite() && (0.0..=100.0).contains(&duty)
}

fn shape_error(name: &str, preset: &CurvePreset) -> Option<String> {
    let label = format!("preset '{name}'");
    match preset {
        CurvePreset::Point { points, .. } => {
            if points.is_empty() {
                return Some(format!("{label}: at least one point is required"));
            }
            if points.iter().any(|[_, duty]| !duty_in_range(*duty)) {
                return Some(format!(
                    "{label}: every point duty must be between 0 and 100"
                ));
            }
            None
        }
        CurvePreset::Flat { duty } => {
            (!duty_in_range(*duty)).then(|| format!("{label}: duty must be between 0 and 100"))
        }
        CurvePreset::Linear {
            min_duty, max_duty, ..
        } => (!duty_in_range(*min_duty) || !duty_in_range(*max_duty))
            .then(|| format!("{label}: min_duty and max_duty must be between 0 and 100")),
        CurvePreset::Trigger {
            on_duty, off_duty, ..
        } => (!duty_in_range(*on_duty) || !duty_in_range(*off_duty))
            .then(|| format!("{label}: on_duty and off_duty must be between 0 and 100")),
        CurvePreset::Target {
            min_duty, max_duty, ..
        } => {
            if !duty_in_range(*min_duty) || !duty_in_range(*max_duty) {
                return Some(format!(
                    "{label}: min_duty and max_duty must be between 0 and 100"
                ));
            }
            (min_duty > max_duty).then(|| format!("{label}: min_duty must not exceed max_duty"))
        }
    }
}

pub fn validate_preset(name: &str, preset: &CurvePreset) -> Result<(), ConfigError> {
    if name.is_empty() {
        return Err(ConfigError::Invalid(
            "preset name must not be empty".to_string(),
        ));
    }
    if name.contains('/') {
        return Err(ConfigError::Invalid(format!(
            "preset '{name}': name must not contain '/'"
        )));
    }
    if is_builtin_name(name) {
        return Err(ConfigError::Invalid(format!(
            "preset '{name}' is built in and cannot be overwritten or deleted"
        )));
    }
    match shape_error(name, preset) {
        Some(message) => Err(ConfigError::Invalid(message)),
        None => Ok(()),
    }
}

pub fn validate_presets(config: &GaleConfig) -> Result<(), ConfigError> {
    for (name, preset) in &config.presets {
        validate_preset(name, preset)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_the_three_named_shapes_and_all_validate() {
        let builtin = builtin();
        assert_eq!(
            builtin.keys().cloned().collect::<Vec<_>>(),
            vec!["Balanced", "Performance", "Quiet"]
        );
        for (name, preset) in &builtin {
            assert!(validate_preset(name, preset).is_err());
            assert_eq!(shape_error(name, preset), None, "{name}");
        }
    }

    #[test]
    fn user_preset_may_not_shadow_a_builtin_in_any_case() {
        let preset = CurvePreset::Flat { duty: 40.0 };
        let error = validate_preset("quiet", &preset).unwrap_err();
        assert!(error.to_string().contains("built in"), "{error}");
        assert!(validate_preset("my_quiet", &preset).is_ok());
    }

    #[test]
    fn preset_name_rules() {
        let preset = CurvePreset::Flat { duty: 40.0 };
        assert!(validate_preset("", &preset).is_err());
        assert!(validate_preset("a/b", &preset).is_err());
    }

    #[test]
    fn preset_shape_rules() {
        assert!(validate_preset(
            "p",
            &CurvePreset::Point {
                points: vec![],
                hysteresis: None,
                response: None
            }
        )
        .is_err());
        assert!(validate_preset(
            "p",
            &CurvePreset::Point {
                points: vec![[30.0, 120.0]],
                hysteresis: None,
                response: None
            }
        )
        .is_err());
        assert!(validate_preset("f", &CurvePreset::Flat { duty: -1.0 }).is_err());
        assert!(validate_preset(
            "t",
            &CurvePreset::Target {
                target_temp: 60.0,
                step_pct_per_sec: 5.0,
                min_duty: 80.0,
                max_duty: 20.0,
                deadband: None,
                idle_temp: None
            }
        )
        .is_err());
        assert!(validate_preset(
            "l",
            &CurvePreset::Linear {
                min_temp: 40.0,
                max_temp: 80.0,
                min_duty: 20.0,
                max_duty: 101.0,
                hysteresis: None,
                response: None
            }
        )
        .is_err());
        assert!(validate_preset(
            "g",
            &CurvePreset::Trigger {
                on_temp: 60.0,
                off_temp: 50.0,
                on_duty: 100.0,
                off_duty: 20.0,
                response: None
            }
        )
        .is_ok());
    }

    #[test]
    fn presets_table_round_trips_through_toml() {
        let toml = r#"
active_profile = "default"

[profiles.default]

[presets.silent_case]
type = "point"
points = [[30.0, 20.0], [60.0, 40.0], [80.0, 100.0]]

[presets.pinned]
type = "flat"
duty = 35.0
"#;
        let config = GaleConfig::from_toml(toml).unwrap();
        assert_eq!(config.presets.len(), 2);
        assert!(matches!(config.presets["pinned"], CurvePreset::Flat { duty } if duty == 35.0));
        let rendered = config.to_toml().unwrap();
        assert!(rendered.contains("[presets.silent_case]"), "{rendered}");
        assert_eq!(GaleConfig::from_toml(&rendered).unwrap(), config);
    }

    #[test]
    fn config_without_presets_table_has_none() {
        let config = GaleConfig::from_toml("active_profile = \"d\"\n[profiles.d]\n").unwrap();
        assert!(config.presets.is_empty());
        assert!(validate_presets(&config).is_ok());
    }
}
