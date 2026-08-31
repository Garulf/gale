use crate::curve::{Curve, EvalContext};
use crate::Id;

const DEADBAND: f64 = 0.5;

pub struct TargetCurve {
    sensor: Id,
    target_temp: f64,
    step_pct_per_sec: f64,
    min_duty: f64,
    max_duty: f64,
    duty: f64,
}

impl TargetCurve {
    pub fn new(sensor: Id, target_temp: f64, step_pct_per_sec: f64, min_duty: f64, max_duty: f64) -> Self {
        Self { sensor, target_temp, step_pct_per_sec, min_duty, max_duty, duty: min_duty }
    }
}

impl Curve for TargetCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        let temp = ctx.sensor(&self.sensor)?;
        let step = self.step_pct_per_sec * ctx.dt_secs;
        if temp > self.target_temp + DEADBAND {
            self.duty += step;
        } else if temp < self.target_temp - DEADBAND {
            self.duty -= step;
        }
        self.duty = self.duty.clamp(self.min_duty, self.max_duty);
        Some(self.duty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{Curve, CurveSet, EvalContext};
    use std::collections::HashMap;

    fn eval(c: &mut TargetCurve, temp: Option<f64>) -> Option<f64> {
        let set = CurveSet::new();
        let sensors: HashMap<_, _> = [("t".to_string(), temp)].into();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        c.evaluate(&ctx)
    }

    #[test]
    fn steps_up_when_hot_down_when_cool_holds_in_deadband() {
        let mut c = TargetCurve::new("t".into(), 60.0, 5.0, 20.0, 100.0);
        assert_eq!(eval(&mut c, Some(70.0)), Some(25.0));
        assert_eq!(eval(&mut c, Some(70.0)), Some(30.0));
        assert_eq!(eval(&mut c, Some(60.2)), Some(30.0));
        assert_eq!(eval(&mut c, Some(50.0)), Some(25.0));
    }

    #[test]
    fn clamps_to_min_and_max_duty() {
        let mut c = TargetCurve::new("t".into(), 60.0, 50.0, 20.0, 60.0);
        assert_eq!(eval(&mut c, Some(90.0)), Some(60.0));
        assert_eq!(eval(&mut c, Some(90.0)), Some(60.0));
        assert_eq!(eval(&mut c, Some(30.0)), Some(20.0));
    }

    #[test]
    fn unavailable_sensor_returns_none_and_holds_duty() {
        let mut c = TargetCurve::new("t".into(), 60.0, 5.0, 20.0, 100.0);
        assert_eq!(eval(&mut c, Some(70.0)), Some(25.0));
        assert_eq!(eval(&mut c, None), None);
        assert_eq!(eval(&mut c, Some(70.0)), Some(30.0));
    }
}
