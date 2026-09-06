use crate::config::ControlSettings;
use crate::curve::{CurveSet, EvalContext};
use crate::r#virtual::VirtualSensors;
use crate::Id;
use std::collections::HashMap;

pub const FAIL_SAFE_PCT: f64 = 100.0;

pub struct FanEngine {
    curves: CurveSet,
    assignments: HashMap<Id, Id>,
    virtual_sensors: VirtualSensors,
    control_settings: HashMap<Id, ControlSettings>,
    last_duties: HashMap<Id, f64>,
    last_curve_outputs: HashMap<Id, f64>,
}

impl std::fmt::Debug for FanEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FanEngine")
            .field("assignments", &self.assignments)
            .finish()
    }
}

impl FanEngine {
    pub fn new(curves: CurveSet, assignments: HashMap<Id, Id>) -> Self {
        Self {
            curves,
            assignments,
            virtual_sensors: VirtualSensors::empty(),
            control_settings: HashMap::new(),
            last_duties: HashMap::new(),
            last_curve_outputs: HashMap::new(),
        }
    }

    pub fn with_control_settings(mut self, settings: HashMap<Id, ControlSettings>) -> Self {
        self.control_settings = settings;
        self
    }

    pub fn with_virtual_sensors(mut self, virtual_sensors: VirtualSensors) -> Self {
        self.virtual_sensors = virtual_sensors;
        self
    }

    pub fn assignments(&self) -> &HashMap<Id, Id> {
        &self.assignments
    }

    pub fn virtual_sensor_ids(&self) -> Vec<Id> {
        self.virtual_sensors.ids()
    }

    pub fn curve_outputs(&self) -> &HashMap<Id, f64> {
        &self.last_curve_outputs
    }

    pub fn tick(
        &mut self,
        sensors: &mut HashMap<Id, Option<f64>>,
        dt_secs: f64,
    ) -> HashMap<Id, f64> {
        self.virtual_sensors.evaluate(sensors, dt_secs);
        let ctx = EvalContext::new(&self.curves, sensors, dt_secs);
        let duties: HashMap<Id, f64> = self
            .assignments
            .iter()
            .map(|(control, curve)| {
                let requested = ctx
                    .resolve(curve)
                    .filter(|d| d.is_finite())
                    .unwrap_or(FAIL_SAFE_PCT)
                    .clamp(0.0, 100.0);
                let duty = match self.control_settings.get(control) {
                    Some(settings) => settings
                        .shape(
                            requested,
                            self.last_duties.get(control).copied().unwrap_or(0.0),
                        )
                        .clamp(0.0, 100.0),
                    None => requested,
                };
                (control.clone(), duty)
            })
            .collect();
        self.last_curve_outputs = self
            .curves
            .ids()
            .filter_map(|id| {
                ctx.resolve(id)
                    .filter(|d| d.is_finite())
                    .map(|d| (id.clone(), d.clamp(0.0, 100.0)))
            })
            .collect();
        self.last_duties = duties.clone();
        duties
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProfileConfig;
    use crate::curve::flat::FlatCurve;
    use crate::curve::point::PointCurve;
    use crate::curve::target::TargetCurve;
    use crate::curve::CurveSet;
    use std::collections::HashMap;

    fn sensors(pairs: &[(&str, Option<f64>)]) -> HashMap<String, Option<f64>> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn tick_returns_duty_for_every_assigned_control() {
        let mut set = CurveSet::new();
        set.insert("quiet".into(), Box::new(FlatCurve { duty: 30.0 }));
        let assignments: HashMap<String, String> = [
            ("pwm1".to_string(), "quiet".to_string()),
            ("pwm2".to_string(), "quiet".to_string()),
        ]
        .into();
        let mut engine = FanEngine::new(set, assignments);
        let duties = engine.tick(&mut sensors(&[]), 1.0);
        assert_eq!(duties["pwm1"], 30.0);
        assert_eq!(duties["pwm2"], 30.0);
    }

    #[test]
    fn unresolved_curve_fails_safe_to_100() {
        let mut set = CurveSet::new();
        set.insert(
            "cpu".into(),
            Box::new(PointCurve::new(
                "t".into(),
                vec![(30.0, 20.0), (70.0, 100.0)],
            )),
        );
        let assignments: HashMap<String, String> = [
            ("pwm1".to_string(), "cpu".to_string()),
            ("pwm2".to_string(), "ghost".to_string()),
        ]
        .into();
        let mut engine = FanEngine::new(set, assignments);
        let duties = engine.tick(&mut sensors(&[("t", None)]), 1.0);
        assert_eq!(duties["pwm1"], FAIL_SAFE_PCT);
        assert_eq!(duties["pwm2"], FAIL_SAFE_PCT);
    }

    #[test]
    fn duties_are_clamped_to_0_100() {
        let mut set = CurveSet::new();
        set.insert("hot".into(), Box::new(FlatCurve { duty: 250.0 }));
        set.insert("neg".into(), Box::new(FlatCurve { duty: -10.0 }));
        let assignments: HashMap<String, String> = [
            ("pwm1".to_string(), "hot".to_string()),
            ("pwm2".to_string(), "neg".to_string()),
        ]
        .into();
        let mut engine = FanEngine::new(set, assignments);
        let duties = engine.tick(&mut sensors(&[]), 1.0);
        assert_eq!(duties["pwm1"], 100.0);
        assert_eq!(duties["pwm2"], 0.0);
    }

    #[test]
    fn nan_duty_fails_safe() {
        let mut set = CurveSet::new();
        set.insert("broken".into(), Box::new(FlatCurve { duty: f64::NAN }));
        let assignments: HashMap<String, String> =
            [("pwm1".to_string(), "broken".to_string())].into();
        let mut engine = FanEngine::new(set, assignments);
        let duties = engine.tick(&mut sensors(&[]), 1.0);
        assert_eq!(duties["pwm1"], FAIL_SAFE_PCT);
    }

    #[test]
    fn engine_without_virtual_sensors_leaves_map_unchanged() {
        let mut set = CurveSet::new();
        set.insert("quiet".into(), Box::new(FlatCurve { duty: 30.0 }));
        let assignments: HashMap<String, String> = [
            ("pwm1".to_string(), "quiet".to_string()),
            ("pwm2".to_string(), "quiet".to_string()),
        ]
        .into();
        let mut engine = FanEngine::new(set, assignments);
        let mut map = sensors(&[]);
        let before_len = map.len();
        let duties = engine.tick(&mut map, 1.0);
        assert_eq!(duties["pwm1"], 30.0);
        assert_eq!(duties["pwm2"], 30.0);
        assert_eq!(map.len(), before_len);
    }

    #[test]
    fn virtual_max_drives_point_curve_through_input_dropout() {
        let profile: ProfileConfig = toml::from_str(
            r#"
            [sensors.cpu_hot]
            type = "max"
            inputs = ["hw/cpu", "hw/gpu"]
            "#,
        )
        .unwrap();
        let virtual_sensors = crate::r#virtual::VirtualSensors::build(&profile).unwrap();

        let mut set = CurveSet::new();
        set.insert(
            "cpu".into(),
            Box::new(PointCurve::new(
                "virtual/cpu_hot".into(),
                vec![(30.0, 20.0), (70.0, 100.0)],
            )),
        );
        let assignments: HashMap<String, String> = [("pwm1".to_string(), "cpu".to_string())].into();
        let mut engine = FanEngine::new(set, assignments).with_virtual_sensors(virtual_sensors);

        let mut map = sensors(&[("hw/cpu", Some(50.0)), ("hw/gpu", Some(60.0))]);
        let duties = engine.tick(&mut map, 1.0);
        assert_eq!(map["virtual/cpu_hot"], Some(60.0));
        assert_eq!(duties["pwm1"], 80.0);

        let mut map = sensors(&[("hw/cpu", Some(65.0)), ("hw/gpu", Some(40.0))]);
        let duties = engine.tick(&mut map, 1.0);
        assert_eq!(map["virtual/cpu_hot"], Some(65.0));
        assert_eq!(duties["pwm1"], 90.0);

        let mut map = sensors(&[("hw/cpu", None), ("hw/gpu", Some(40.0))]);
        let duties = engine.tick(&mut map, 1.0);
        assert_eq!(map["virtual/cpu_hot"], Some(40.0));
        assert_eq!(duties["pwm1"], 40.0);

        let mut map = sensors(&[("hw/cpu", None), ("hw/gpu", None)]);
        let duties = engine.tick(&mut map, 1.0);
        assert_eq!(map["virtual/cpu_hot"], None);
        assert_eq!(duties["pwm1"], FAIL_SAFE_PCT);

        let mut map = sensors(&[("hw/cpu", Some(45.0))]);
        let duties = engine.tick(&mut map, 1.0);
        assert_eq!(map["virtual/cpu_hot"], Some(45.0));
        assert_eq!(duties["pwm1"], 50.0);
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn webhook_seed_drives_point_curve_and_fails_safe_when_missing() {
        let profile: ProfileConfig = toml::from_str(&format!(
            r#"
            [sensors.hook]
            type = "webhook"
            token = "{TOKEN}"
            "#,
        ))
        .unwrap();
        let virtual_sensors = crate::r#virtual::VirtualSensors::build(&profile).unwrap();

        let mut set = CurveSet::new();
        set.insert(
            "remote".into(),
            Box::new(PointCurve::new(
                "virtual/hook".into(),
                vec![(30.0, 20.0), (70.0, 100.0)],
            )),
        );
        let assignments: HashMap<String, String> =
            [("pwm1".to_string(), "remote".to_string())].into();
        let mut engine = FanEngine::new(set, assignments).with_virtual_sensors(virtual_sensors);

        let duties = engine.tick(&mut sensors(&[("virtual/hook", Some(50.0))]), 1.0);
        assert_eq!(duties["pwm1"], 60.0);

        let duties = engine.tick(&mut sensors(&[("virtual/hook", None)]), 1.0);
        assert_eq!(duties["pwm1"], FAIL_SAFE_PCT);

        let duties = engine.tick(&mut sensors(&[]), 1.0);
        assert_eq!(duties["pwm1"], FAIL_SAFE_PCT);
    }

    #[test]
    fn engine_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<FanEngine>();
    }

    #[test]
    fn control_settings_apply_stop_start_and_minimum() {
        let mut set = CurveSet::new();
        set.insert(
            "cpu".into(),
            Box::new(PointCurve::new(
                "t".into(),
                vec![(30.0, 10.0), (70.0, 100.0)],
            )),
        );
        let assignments: HashMap<String, String> = [("pwm1".to_string(), "cpu".to_string())].into();
        let settings: HashMap<String, ControlSettings> = [(
            "pwm1".to_string(),
            ControlSettings {
                min_duty: None,
                start_duty: Some(40.0),
                stop_duty: Some(15.0),
            },
        )]
        .into();
        let mut engine = FanEngine::new(set, assignments).with_control_settings(settings);
        assert_eq!(
            engine.tick(&mut sensors(&[("t", Some(30.0))]), 1.0)["pwm1"],
            0.0
        );
        assert_eq!(
            engine.tick(&mut sensors(&[("t", Some(40.0))]), 1.0)["pwm1"],
            40.0
        );
        assert_eq!(
            engine.tick(&mut sensors(&[("t", Some(40.0))]), 1.0)["pwm1"],
            32.5
        );
        assert_eq!(
            engine.tick(&mut sensors(&[("t", Some(30.0))]), 1.0)["pwm1"],
            0.0
        );
    }

    #[test]
    fn curve_outputs_cover_every_curve_including_unassigned_ones() {
        let mut set = CurveSet::new();
        set.insert("fast".into(), Box::new(FlatCurve { duty: 70.0 }));
        set.insert("slow".into(), Box::new(FlatCurve { duty: 20.0 }));
        let assignments: HashMap<String, String> =
            [("pwm1".to_string(), "fast".to_string())].into();
        let mut engine = FanEngine::new(set, assignments);
        assert!(engine.curve_outputs().is_empty());

        engine.tick(&mut sensors(&[]), 1.0);
        assert_eq!(engine.curve_outputs()["fast"], 70.0);
        assert_eq!(engine.curve_outputs()["slow"], 20.0);
    }

    #[test]
    fn curve_outputs_omit_curves_that_have_no_value() {
        let mut set = CurveSet::new();
        set.insert(
            "cpu".into(),
            Box::new(PointCurve::new(
                "t".into(),
                vec![(30.0, 20.0), (70.0, 100.0)],
            )),
        );
        let mut engine = FanEngine::new(set, HashMap::new());
        engine.tick(&mut sensors(&[("t", None)]), 1.0);
        assert!(!engine.curve_outputs().contains_key("cpu"));

        engine.tick(&mut sensors(&[("t", Some(50.0))]), 1.0);
        assert_eq!(engine.curve_outputs()["cpu"], 60.0);
    }

    #[test]
    fn unassigned_stateful_curves_track_live_conditions_every_tick() {
        let mut set = CurveSet::new();
        set.insert(
            "hold".into(),
            Box::new(TargetCurve::new("t".into(), 40.0, 10.0, 20.0, 60.0)),
        );
        let mut engine = FanEngine::new(set, HashMap::new());
        engine.tick(&mut sensors(&[("t", Some(50.0))]), 1.0);
        let first = engine.curve_outputs()["hold"];
        engine.tick(&mut sensors(&[("t", Some(50.0))]), 1.0);
        let second = engine.curve_outputs()["hold"];
        assert!(
            second > first,
            "a target curve above its setpoint keeps ramping while unassigned"
        );
    }

    #[test]
    fn minimum_duty_is_a_hard_floor_even_below_stop() {
        let mut set = CurveSet::new();
        set.insert("low".into(), Box::new(FlatCurve { duty: 5.0 }));
        let assignments: HashMap<String, String> = [("pwm1".to_string(), "low".to_string())].into();
        let settings: HashMap<String, ControlSettings> = [(
            "pwm1".to_string(),
            ControlSettings {
                min_duty: Some(25.0),
                start_duty: None,
                stop_duty: Some(10.0),
            },
        )]
        .into();
        let mut engine = FanEngine::new(set, assignments).with_control_settings(settings);
        assert_eq!(engine.tick(&mut sensors(&[]), 1.0)["pwm1"], 25.0);
    }
}
