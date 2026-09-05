use crate::config::{
    virtual_name, ConfigError, CorsairReleaseMode, CurveConfig, GaleConfig, ProfileConfig,
};
use crate::curve::flat::FlatCurve;
use crate::curve::linear::LinearCurve;
use crate::curve::mix::MixCurve;
use crate::curve::offset::OffsetCurve;
use crate::curve::point::PointCurve;
use crate::curve::sync::SyncCurve;
use crate::curve::target::TargetCurve;
use crate::curve::trigger::TriggerCurve;
use crate::curve::{Curve, CurveSet};
use crate::engine::FanEngine;
use crate::presets::validate_presets;
use crate::r#virtual::VirtualSensors;
use crate::Id;

pub fn build_engine(config: &GaleConfig) -> Result<FanEngine, ConfigError> {
    validate_hardware(config)?;
    validate_presets(config)?;

    let profile = config.profiles.get(&config.active_profile).ok_or_else(|| {
        ConfigError::Invalid(format!(
            "active_profile '{}' is not defined",
            config.active_profile
        ))
    })?;

    let virtual_sensors = validate_profile(&config.active_profile, profile)?;

    let mut set = CurveSet::new();
    for (id, curve) in &profile.curves {
        set.insert(id.clone(), instantiate(curve));
    }
    Ok(
        FanEngine::new(set, profile.assignments.clone().into_iter().collect())
            .with_virtual_sensors(virtual_sensors)
            .with_control_settings(config.controls.clone().into_iter().collect()),
    )
}

pub fn validate_profiles(config: &GaleConfig) -> Result<(), ConfigError> {
    validate_hardware(config)?;
    validate_presets(config)?;
    if !config.profiles.contains_key(&config.active_profile) {
        return Err(ConfigError::Invalid(format!(
            "active_profile '{}' is not defined",
            config.active_profile
        )));
    }
    for (name, profile) in &config.profiles {
        validate_profile(name, profile)?;
    }
    Ok(())
}

fn validate_hardware(config: &GaleConfig) -> Result<(), ConfigError> {
    if let CorsairReleaseMode::Fixed { percent } = config.hardware.corsair.on_release {
        if percent > 100 {
            return Err(ConfigError::Invalid(format!(
                "hardware.corsair.on_release fixed percent {percent} must be between 0 and 100"
            )));
        }
    }
    for (control, settings) in &config.controls {
        for (field, value) in [
            ("min_duty", settings.min_duty),
            ("start_duty", settings.start_duty),
            ("stop_duty", settings.stop_duty),
        ] {
            if let Some(value) = value {
                if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                    return Err(ConfigError::Invalid(format!(
                        "controls.'{control}'.{field} must be between 0 and 100"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_profile(name: &str, profile: &ProfileConfig) -> Result<VirtualSensors, ConfigError> {
    check_profile(profile).map_err(|error| match error {
        ConfigError::Invalid(message) => {
            ConfigError::Invalid(format!("profile '{name}': {message}"))
        }
        other => other,
    })
}

fn check_profile(profile: &ProfileConfig) -> Result<VirtualSensors, ConfigError> {
    for (id, curve) in &profile.curves {
        for source in curve_references(curve) {
            if !profile.curves.contains_key(source) {
                return Err(ConfigError::Invalid(format!(
                    "curve '{id}' references undefined curve '{source}'"
                )));
            }
        }
    }
    for (control, curve_id) in &profile.assignments {
        if !profile.curves.contains_key(curve_id) {
            return Err(ConfigError::Invalid(format!(
                "control '{control}' is assigned undefined curve '{curve_id}'"
            )));
        }
    }
    for (curve_id, curve) in &profile.curves {
        if let Some(sensor) = curve_sensor(curve) {
            if let Some(name) = virtual_name(sensor) {
                if !profile.sensors.contains_key(name) {
                    return Err(ConfigError::Invalid(format!(
                        "curve '{curve_id}' reads undefined virtual sensor '{sensor}'"
                    )));
                }
            }
        }
    }
    VirtualSensors::build(profile)
}

fn curve_references(curve: &CurveConfig) -> Vec<&str> {
    match curve {
        CurveConfig::Mix { sources, .. } => sources.iter().map(String::as_str).collect(),
        CurveConfig::Sync { source } | CurveConfig::Offset { source, .. } => vec![source.as_str()],
        _ => Vec::new(),
    }
}

fn curve_sensor(curve: &CurveConfig) -> Option<&Id> {
    match curve {
        CurveConfig::Point { sensor, .. } => Some(sensor),
        CurveConfig::Linear { sensor, .. } => Some(sensor),
        CurveConfig::Trigger { sensor, .. } => Some(sensor),
        CurveConfig::Target { sensor, .. } => Some(sensor),
        _ => None,
    }
}

fn instantiate(curve: &CurveConfig) -> Box<dyn Curve> {
    match curve {
        CurveConfig::Point {
            sensor,
            points,
            hysteresis,
            response,
        } => {
            let mut c = PointCurve::new(
                sensor.clone(),
                points.iter().map(|p| (p[0], p[1])).collect(),
            );
            if let Some(h) = hysteresis {
                c = c.with_hysteresis(h.up, h.down);
            }
            if let Some(r) = response {
                c = c.with_response(r.rise_pct_per_sec, r.fall_pct_per_sec);
            }
            Box::new(c)
        }
        CurveConfig::Flat { duty } => Box::new(FlatCurve { duty: *duty }),
        CurveConfig::Linear {
            sensor,
            min_temp,
            max_temp,
            min_duty,
            max_duty,
            hysteresis,
            response,
        } => {
            let mut c =
                LinearCurve::new(sensor.clone(), *min_temp, *max_temp, *min_duty, *max_duty);
            if let Some(h) = hysteresis {
                c = c.with_hysteresis(h.up, h.down);
            }
            if let Some(r) = response {
                c = c.with_response(r.rise_pct_per_sec, r.fall_pct_per_sec);
            }
            Box::new(c)
        }
        CurveConfig::Mix { sources, mode } => Box::new(MixCurve {
            sources: sources.clone(),
            mode: *mode,
        }),
        CurveConfig::Sync { source } => Box::new(SyncCurve {
            source: source.clone(),
        }),
        CurveConfig::Offset { source, add, scale } => Box::new(OffsetCurve {
            source: source.clone(),
            add: *add,
            scale: *scale,
        }),
        CurveConfig::Trigger {
            sensor,
            on_temp,
            off_temp,
            on_duty,
            off_duty,
            response,
        } => {
            let mut c = TriggerCurve::new(sensor.clone(), *on_temp, *off_temp, *on_duty, *off_duty);
            if let Some(r) = response {
                c = c.with_response(r.rise_pct_per_sec, r.fall_pct_per_sec);
            }
            Box::new(c)
        }
        CurveConfig::Target {
            sensor,
            target_temp,
            step_pct_per_sec,
            min_duty,
            max_duty,
            deadband,
            idle_temp,
        } => {
            let mut c = TargetCurve::new(
                sensor.clone(),
                *target_temp,
                *step_pct_per_sec,
                *min_duty,
                *max_duty,
            );
            if let Some(deadband) = deadband {
                c = c.with_deadband(*deadband);
            }
            if let Some(idle) = idle_temp {
                c = c.with_idle_temp(*idle);
            }
            Box::new(c)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GaleConfig;
    use std::collections::HashMap;

    fn config(toml_str: &str) -> GaleConfig {
        GaleConfig::from_toml(toml_str).unwrap()
    }

    const VALID: &str = r#"
active_profile = "quiet"

[profiles.quiet.curves.cpu]
type = "point"
sensor = "hwmon/nct6798/temp1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.quiet.curves.cpu.hysteresis]
up = 2.0
down = 5.0

[profiles.quiet.curves.cpu.response]
rise_pct_per_sec = 10.0
fall_pct_per_sec = 20.0

[profiles.quiet.curves.fixed]
type = "flat"
duty = 40.0

[profiles.quiet.curves.combined]
type = "mix"
sources = ["cpu", "fixed"]
mode = "max"

[profiles.quiet.curves.mirror]
type = "sync"
source = "combined"

[profiles.quiet.curves.emergency]
type = "trigger"
sensor = "hwmon/nct6798/temp1"
on_temp = 80.0
off_temp = 70.0
on_duty = 100.0
off_duty = 0.0

[profiles.quiet.curves.pump]
type = "target"
sensor = "hwmon/nct6798/temp1"
target_temp = 60.0
step_pct_per_sec = 5.0
min_duty = 20.0
max_duty = 100.0

[profiles.quiet.assignments]
"hwmon/nct6798/pwm1" = "mirror"
"#;

    #[test]
    fn builds_engine_from_valid_config_and_ticks() {
        let mut engine = build_engine(&config(VALID)).unwrap();
        let mut sensors: HashMap<String, Option<f64>> =
            [("hwmon/nct6798/temp1".to_string(), Some(50.0))].into();
        let duties = engine.tick(&mut sensors, 1.0);
        assert_eq!(duties["hwmon/nct6798/pwm1"], 60.0);
    }

    #[test]
    fn unknown_active_profile_is_invalid() {
        let mut cfg = config(VALID);
        cfg.active_profile = "ghost".into();
        assert!(matches!(build_engine(&cfg), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn corsair_fixed_release_percent_above_hundred_is_invalid() {
        let mut cfg = config(VALID);
        cfg.hardware.corsair.on_release = crate::config::CorsairReleaseMode::Fixed { percent: 101 };
        assert!(matches!(build_engine(&cfg), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn dangling_curve_reference_is_invalid() {
        let bad = r#"
active_profile = "p"

[profiles.p.curves.m]
type = "sync"
source = "missing"

[profiles.p.assignments]
"pwm1" = "m"
"#;
        let err = build_engine(&config(bad)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("m") && msg.contains("missing"), "{msg}");
    }

    #[test]
    fn dangling_assignment_is_invalid() {
        let bad = r#"
active_profile = "p"

[profiles.p.curves.c]
type = "flat"
duty = 50.0

[profiles.p.assignments]
"pwm1" = "ghost"
"#;
        assert!(matches!(
            build_engine(&config(bad)),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn builds_engine_with_virtual_sensors_from_toml() {
        let (before_assignments, assignments_and_after) = VALID
            .split_once("[profiles.quiet.assignments]")
            .expect("VALID has an assignments table");
        let with_virtual = format!(
            r#"{before_assignments}
[profiles.quiet.sensors.cpu_hot]
type = "max"
inputs = ["superio/nct6798d/temp2", "nvidia/0/temp"]

[profiles.quiet.sensors.coolant_smooth]
type = "mean"
inputs = ["corsair/commander-pro-0805009c9327/temp1"]
window_s = 10

[profiles.quiet.sensors.coolant_rise]
type = "delta"
input = "virtual/coolant_smooth"
window_s = 30

[profiles.quiet.sensors.gpu_adjusted]
type = "offset"
input = "nvidia/0/temp"
add = -5.0
scale = 1.0

[profiles.quiet.curves.hot]
type = "point"
sensor = "virtual/cpu_hot"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.quiet.assignments]{assignments_and_after}
"hwmon/nct6798/pwm2" = "hot"
"#
        );
        let mut engine = build_engine(&config(&with_virtual)).unwrap();
        let mut sensors: HashMap<String, Option<f64>> = [
            ("superio/nct6798d/temp2".to_string(), Some(55.0)),
            ("nvidia/0/temp".to_string(), Some(61.0)),
        ]
        .into();
        let duties = engine.tick(&mut sensors, 1.0);
        assert_eq!(duties["hwmon/nct6798/pwm2"], 82.0);
        assert_eq!(engine.virtual_sensor_ids().len(), 4);
    }

    #[test]
    fn curve_on_undefined_virtual_sensor_is_invalid() {
        let bad = r#"
active_profile = "p"

[profiles.p.curves.c]
type = "point"
sensor = "virtual/ghost"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.assignments]
"pwm1" = "c"
"#;
        let err = build_engine(&config(bad)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("virtual/ghost"), "{msg}");
    }

    #[test]
    fn validate_profiles_names_the_broken_profile() {
        let cfg = config(&format!(
            r#"{VALID}
[profiles.loud.sensors.a]
type = "max"
inputs = ["virtual/b"]

[profiles.loud.sensors.b]
type = "min"
inputs = ["virtual/a"]

[profiles.silent.curves.c]
type = "flat"
duty = 10.0
"#
        ));
        build_engine(&cfg).expect("active profile alone still builds");
        let err = validate_profiles(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("profile 'loud'"), "{msg}");
        assert!(msg.contains("cycle"), "{msg}");
        assert!(!msg.contains("quiet") && !msg.contains("silent"), "{msg}");
    }

    #[test]
    fn validate_profiles_accepts_config_where_every_profile_is_valid() {
        let cfg = config(&format!(
            r#"{VALID}
[profiles.loud.sensors.hot]
type = "max"
inputs = ["nvidia/0/temp"]

[profiles.loud.curves.c]
type = "point"
sensor = "virtual/hot"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.loud.assignments]
"pwm1" = "c"
"#
        ));
        assert_eq!(validate_profiles(&cfg), Ok(()));
    }

    #[test]
    fn validate_profiles_catches_undefined_references_in_inactive_profile() {
        let dangling_curve = config(&format!(
            r#"{VALID}
[profiles.loud.curves.m]
type = "sync"
source = "missing"
"#
        ));
        let msg = validate_profiles(&dangling_curve).unwrap_err().to_string();
        assert!(
            msg.contains("profile 'loud'") && msg.contains("missing"),
            "{msg}"
        );

        let ghost_virtual = config(&format!(
            r#"{VALID}
[profiles.loud.curves.c]
type = "point"
sensor = "virtual/ghost"
points = [[30.0, 20.0], [70.0, 100.0]]
"#
        ));
        let msg = validate_profiles(&ghost_virtual).unwrap_err().to_string();
        assert!(
            msg.contains("profile 'loud'") && msg.contains("virtual/ghost"),
            "{msg}"
        );
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn builds_engine_with_webhook_sensor_from_toml() {
        let (before_assignments, assignments_and_after) = VALID
            .split_once("[profiles.quiet.assignments]")
            .expect("VALID has an assignments table");
        let with_webhook = format!(
            r#"{before_assignments}
[profiles.quiet.sensors.hook]
type = "webhook"
token = "{TOKEN}"
timeout_s = 60.0

[profiles.quiet.curves.remote]
type = "point"
sensor = "virtual/hook"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.quiet.assignments]{assignments_and_after}
"hwmon/nct6798/pwm2" = "remote"
"#
        );
        let mut engine = build_engine(&config(&with_webhook)).unwrap();
        let mut sensors: HashMap<String, Option<f64>> = [
            ("hwmon/nct6798/temp1".to_string(), Some(50.0)),
            ("virtual/hook".to_string(), Some(50.0)),
        ]
        .into();
        let duties = engine.tick(&mut sensors, 1.0);
        assert_eq!(duties["hwmon/nct6798/pwm2"], 60.0);
        assert_eq!(
            engine.virtual_sensor_ids(),
            vec!["virtual/hook".to_string()]
        );
    }

    #[test]
    fn webhook_with_empty_token_is_invalid_at_build() {
        let bad = format!(
            r#"{VALID}
[profiles.quiet.sensors.hook]
type = "webhook"
"#
        );
        let err = build_engine(&config(&bad)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("profile 'quiet'"), "{msg}");
        assert!(msg.contains("has an empty token"), "{msg}");
    }

    #[test]
    fn virtual_sensor_cycle_is_invalid() {
        let bad = r#"
active_profile = "p"

[profiles.p.sensors.a]
type = "max"
inputs = ["virtual/b"]

[profiles.p.sensors.b]
type = "min"
inputs = ["virtual/a"]

[profiles.p.curves.c]
type = "point"
sensor = "virtual/a"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.assignments]
"pwm1" = "c"
"#;
        assert!(matches!(
            build_engine(&config(bad)),
            Err(ConfigError::Invalid(_))
        ));
    }
}
