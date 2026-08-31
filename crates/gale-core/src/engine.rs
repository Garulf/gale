use crate::curve::{CurveSet, EvalContext};
use crate::Id;
use std::collections::HashMap;

pub const FAIL_SAFE_PCT: f64 = 100.0;

pub struct FanEngine {
    curves: CurveSet,
    assignments: HashMap<Id, Id>,
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
        }
    }

    pub fn assignments(&self) -> &HashMap<Id, Id> {
        &self.assignments
    }

    pub fn tick(&mut self, sensors: &HashMap<Id, Option<f64>>, dt_secs: f64) -> HashMap<Id, f64> {
        let ctx = EvalContext::new(&self.curves, sensors, dt_secs);
        self.assignments
            .iter()
            .map(|(control, curve)| {
                let duty = ctx
                    .resolve(curve)
                    .filter(|d| d.is_finite())
                    .unwrap_or(FAIL_SAFE_PCT);
                (control.clone(), duty.clamp(0.0, 100.0))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::flat::FlatCurve;
    use crate::curve::point::PointCurve;
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
        let duties = engine.tick(&sensors(&[]), 1.0);
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
        let duties = engine.tick(&sensors(&[("t", None)]), 1.0);
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
        let duties = engine.tick(&sensors(&[]), 1.0);
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
        let duties = engine.tick(&sensors(&[]), 1.0);
        assert_eq!(duties["pwm1"], FAIL_SAFE_PCT);
    }

    #[test]
    fn engine_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<FanEngine>();
    }
}
