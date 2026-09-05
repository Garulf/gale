use crate::curve::{Curve, EvalContext};
use crate::Id;

const DEFAULT_DEADBAND: f64 = 0.5;

pub struct TargetCurve {
    sensor: Id,
    target_temp: f64,
    step_pct_per_sec: f64,
    min_duty: f64,
    max_duty: f64,
    deadband: f64,
    idle_temp: Option<f64>,
    duty: f64,
}

impl TargetCurve {
    pub fn new(
        sensor: Id,
        target_temp: f64,
        step_pct_per_sec: f64,
        min_duty: f64,
        max_duty: f64,
    ) -> Self {
        Self {
            sensor,
            target_temp,
            step_pct_per_sec,
            min_duty,
            max_duty,
            deadband: DEFAULT_DEADBAND,
            idle_temp: None,
            duty: min_duty,
        }
    }

    pub fn with_deadband(mut self, deadband: f64) -> Self {
        self.deadband = deadband.max(0.0);
        self
    }

    pub fn with_idle_temp(mut self, idle_temp: f64) -> Self {
        self.idle_temp = Some(idle_temp);
        self
    }
}

impl Curve for TargetCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        let temp = ctx.sensor(&self.sensor)?;
        if self.idle_temp.is_some_and(|idle| temp <= idle) {
            self.duty = self.min_duty;
            return Some(self.duty);
        }
        let step = self.step_pct_per_sec * ctx.dt_secs;
        if temp > self.target_temp + self.deadband {
            self.duty += step;
        } else if temp < self.target_temp - self.deadband {
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
    fn deadband_widens_the_hold_zone() {
        let mut c = TargetCurve::new("t".into(), 60.0, 5.0, 20.0, 100.0).with_deadband(3.0);
        assert_eq!(eval(&mut c, Some(62.5)), Some(20.0));
        assert_eq!(eval(&mut c, Some(63.5)), Some(25.0));
        assert_eq!(eval(&mut c, Some(56.5)), Some(20.0));
    }

    #[test]
    fn idle_temp_drops_straight_to_min_duty_and_restarts_from_there() {
        let mut c = TargetCurve::new("t".into(), 60.0, 5.0, 20.0, 100.0).with_idle_temp(40.0);
        assert_eq!(eval(&mut c, Some(70.0)), Some(25.0));
        assert_eq!(eval(&mut c, Some(70.0)), Some(30.0));
        assert_eq!(eval(&mut c, Some(35.0)), Some(20.0));
        assert_eq!(eval(&mut c, Some(70.0)), Some(25.0));
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
