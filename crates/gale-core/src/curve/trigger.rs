use crate::curve::{Curve, EvalContext};
use crate::Id;

pub struct TriggerCurve {
    sensor: Id,
    on_temp: f64,
    off_temp: f64,
    on_duty: f64,
    off_duty: f64,
    active: bool,
}

impl TriggerCurve {
    pub fn new(sensor: Id, on_temp: f64, off_temp: f64, on_duty: f64, off_duty: f64) -> Self {
        Self {
            sensor,
            on_temp,
            off_temp,
            on_duty,
            off_duty,
            active: false,
        }
    }
}

impl Curve for TriggerCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        let temp = ctx.sensor(&self.sensor)?;
        if temp >= self.on_temp {
            self.active = true;
        } else if temp <= self.off_temp {
            self.active = false;
        }
        Some(if self.active {
            self.on_duty
        } else {
            self.off_duty
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{Curve, CurveSet, EvalContext};
    use std::collections::HashMap;

    fn eval(c: &mut TriggerCurve, temp: Option<f64>) -> Option<f64> {
        let set = CurveSet::new();
        let sensors: HashMap<_, _> = [("t".to_string(), temp)].into();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        c.evaluate(&ctx)
    }

    #[test]
    fn latches_on_above_on_temp_and_off_below_off_temp() {
        let mut c = TriggerCurve::new("t".into(), 70.0, 55.0, 100.0, 30.0);
        assert_eq!(eval(&mut c, Some(60.0)), Some(30.0));
        assert_eq!(eval(&mut c, Some(70.0)), Some(100.0));
        assert_eq!(eval(&mut c, Some(60.0)), Some(100.0));
        assert_eq!(eval(&mut c, Some(55.0)), Some(30.0));
        assert_eq!(eval(&mut c, Some(60.0)), Some(30.0));
    }

    #[test]
    fn unavailable_sensor_returns_none_and_holds_state() {
        let mut c = TriggerCurve::new("t".into(), 70.0, 55.0, 100.0, 30.0);
        assert_eq!(eval(&mut c, Some(75.0)), Some(100.0));
        assert_eq!(eval(&mut c, None), None);
        assert_eq!(eval(&mut c, Some(60.0)), Some(100.0));
    }
}
