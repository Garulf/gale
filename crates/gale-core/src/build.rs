use crate::config::{ConfigError, CorsairReleaseMode, CurveConfig, GaleConfig};
use crate::curve::flat::FlatCurve;
use crate::curve::mix::MixCurve;
use crate::curve::point::PointCurve;
use crate::curve::sync::SyncCurve;
use crate::curve::target::TargetCurve;
use crate::curve::trigger::TriggerCurve;
use crate::curve::{Curve, CurveSet};
use crate::engine::FanEngine;

pub fn build_engine(config: &GaleConfig) -> Result<FanEngine, ConfigError> {
    if let CorsairReleaseMode::Fixed { percent } = config.hardware.corsair.on_release {
        if percent > 100 {
            return Err(ConfigError::Invalid(format!(
                "hardware.corsair.on_release fixed percent {percent} must be between 0 and 100"
            )));
        }
    }

    let profile = config.profiles.get(&config.active_profile).ok_or_else(|| {
        ConfigError::Invalid(format!(
            "active_profile '{}' is not defined",
            config.active_profile
        ))
    })?;

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

    let mut set = CurveSet::new();
    for (id, curve) in &profile.curves {
        set.insert(id.clone(), instantiate(curve));
    }
    Ok(FanEngine::new(
        set,
        profile.assignments.clone().into_iter().collect(),
    ))
}

fn curve_references(curve: &CurveConfig) -> Vec<&str> {
    match curve {
        CurveConfig::Mix { sources, .. } => sources.iter().map(String::as_str).collect(),
        CurveConfig::Sync { source } => vec![source.as_str()],
        _ => Vec::new(),
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
        CurveConfig::Mix { sources, mode } => Box::new(MixCurve {
            sources: sources.clone(),
            mode: *mode,
        }),
        CurveConfig::Sync { source } => Box::new(SyncCurve {
            source: source.clone(),
        }),
        CurveConfig::Trigger {
            sensor,
            on_temp,
            off_temp,
            on_duty,
            off_duty,
        } => Box::new(TriggerCurve::new(
            sensor.clone(),
            *on_temp,
            *off_temp,
            *on_duty,
            *off_duty,
        )),
        CurveConfig::Target {
            sensor,
            target_temp,
            step_pct_per_sec,
            min_duty,
            max_duty,
        } => Box::new(TargetCurve::new(
            sensor.clone(),
            *target_temp,
            *step_pct_per_sec,
            *min_duty,
            *max_duty,
        )),
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
        let sensors: HashMap<String, Option<f64>> =
            [("hwmon/nct6798/temp1".to_string(), Some(50.0))].into();
        let duties = engine.tick(&sensors, 1.0);
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
}
