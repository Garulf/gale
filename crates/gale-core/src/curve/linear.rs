use crate::curve::{Curve, EvalContext, Hysteresis, ResponseLimit};
use crate::Id;

pub struct LinearCurve {
    sensor: Id,
    min_temp: f64,
    max_temp: f64,
    min_duty: f64,
    max_duty: f64,
    hysteresis: Hysteresis,
    response: ResponseLimit,
}

impl LinearCurve {
    pub fn new(sensor: Id, min_temp: f64, max_temp: f64, min_duty: f64, max_duty: f64) -> Self {
        Self {
            sensor,
            min_temp,
            max_temp,
            min_duty,
            max_duty,
            hysteresis: Hysteresis::default(),
            response: ResponseLimit::default(),
        }
    }

    pub fn with_hysteresis(mut self, up: f64, down: f64) -> Self {
        self.hysteresis = Hysteresis::new(up, down);
        self
    }

    pub fn with_response(mut self, rise_pct_per_sec: f64, fall_pct_per_sec: f64) -> Self {
        self.response = ResponseLimit::new(rise_pct_per_sec, fall_pct_per_sec);
        self
    }

    fn duty_for(&self, temp: f64) -> f64 {
        if temp <= self.min_temp {
            return self.min_duty;
        }
        if temp >= self.max_temp {
            return self.max_duty;
        }
        let span = self.max_temp - self.min_temp;
        self.min_duty + (self.max_duty - self.min_duty) * (temp - self.min_temp) / span
    }
}

impl Curve for LinearCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        let temp = self.hysteresis.apply(ctx.sensor(&self.sensor)?);
        let target = self.duty_for(temp);
        Some(self.response.apply(target, ctx.dt_secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{Curve, CurveSet, EvalContext};
    use std::collections::HashMap;

    fn eval(c: &mut LinearCurve, temp: Option<f64>) -> Option<f64> {
        let set = CurveSet::new();
        let sensors: HashMap<_, _> = [("t".to_string(), temp)].into();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        c.evaluate(&ctx)
    }

    #[test]
    fn interpolates_between_the_two_ends_and_clamps_outside() {
        let mut c = LinearCurve::new("t".into(), 40.0, 80.0, 20.0, 100.0);
        assert_eq!(eval(&mut c, Some(30.0)), Some(20.0));
        assert_eq!(eval(&mut c, Some(40.0)), Some(20.0));
        assert_eq!(eval(&mut c, Some(60.0)), Some(60.0));
        assert_eq!(eval(&mut c, Some(80.0)), Some(100.0));
        assert_eq!(eval(&mut c, Some(95.0)), Some(100.0));
    }

    #[test]
    fn hysteresis_holds_the_effective_temp_inside_the_band() {
        let mut c = LinearCurve::new("t".into(), 40.0, 80.0, 0.0, 100.0).with_hysteresis(2.0, 5.0);
        assert_eq!(eval(&mut c, Some(60.0)), Some(50.0));
        assert_eq!(eval(&mut c, Some(61.5)), Some(50.0));
        assert_eq!(eval(&mut c, Some(56.0)), Some(50.0));
        assert_eq!(eval(&mut c, Some(63.0)), Some(57.5));
    }

    #[test]
    fn response_limits_the_rate_of_change() {
        let mut c = LinearCurve::new("t".into(), 40.0, 80.0, 0.0, 100.0).with_response(10.0, 5.0);
        assert_eq!(eval(&mut c, Some(40.0)), Some(0.0));
        assert_eq!(eval(&mut c, Some(80.0)), Some(10.0));
        assert_eq!(eval(&mut c, Some(80.0)), Some(20.0));
        assert_eq!(eval(&mut c, Some(40.0)), Some(15.0));
    }

    #[test]
    fn missing_sensor_yields_none() {
        let mut c = LinearCurve::new("t".into(), 40.0, 80.0, 20.0, 100.0);
        assert_eq!(eval(&mut c, None), None);
    }

    #[test]
    fn zero_width_range_steps_at_the_threshold() {
        let mut c = LinearCurve::new("t".into(), 60.0, 60.0, 20.0, 100.0);
        assert_eq!(eval(&mut c, Some(59.0)), Some(20.0));
        assert_eq!(eval(&mut c, Some(60.0)), Some(20.0));
        assert_eq!(eval(&mut c, Some(60.5)), Some(100.0));
    }
}
