use crate::curve::{Curve, EvalContext};
use crate::Id;

pub struct PointCurve {
    sensor: Id,
    points: Vec<(f64, f64)>,
    hysteresis: Option<(f64, f64)>,
    effective_temp: Option<f64>,
    response: Option<(f64, f64)>,
    last_output: Option<f64>,
}

impl PointCurve {
    pub fn new(sensor: Id, mut points: Vec<(f64, f64)>) -> Self {
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self {
            sensor,
            points,
            hysteresis: None,
            effective_temp: None,
            response: None,
            last_output: None,
        }
    }

    pub fn with_hysteresis(mut self, up: f64, down: f64) -> Self {
        self.hysteresis = Some((up, down));
        self
    }

    pub fn with_response(mut self, rise_pct_per_sec: f64, fall_pct_per_sec: f64) -> Self {
        self.response = Some((rise_pct_per_sec, fall_pct_per_sec));
        self
    }

    fn apply_response(&mut self, target: f64, dt_secs: f64) -> f64 {
        let Some((rise, fall)) = self.response else {
            return target;
        };
        let output = match self.last_output {
            None => target,
            Some(prev) if target > prev => prev + (target - prev).min(rise * dt_secs),
            Some(prev) => prev - (prev - target).min(fall * dt_secs),
        };
        self.last_output = Some(output);
        output
    }

    fn apply_hysteresis(&mut self, temp: f64) -> f64 {
        let Some((up, down)) = self.hysteresis else {
            return temp;
        };
        match self.effective_temp {
            Some(eff) if temp <= eff + up && temp >= eff - down => eff,
            _ => {
                self.effective_temp = Some(temp);
                temp
            }
        }
    }

    fn duty_for(&self, temp: f64) -> Option<f64> {
        let (first, last) = (self.points.first()?, self.points.last()?);
        if temp <= first.0 {
            return Some(first.1);
        }
        if temp >= last.0 {
            return Some(last.1);
        }
        let upper = self.points.iter().position(|p| p.0 >= temp)?;
        let (t0, d0) = self.points[upper - 1];
        let (t1, d1) = self.points[upper];
        Some(d0 + (d1 - d0) * (temp - t0) / (t1 - t0))
    }
}

impl Curve for PointCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        let temp = self.apply_hysteresis(ctx.sensor(&self.sensor)?);
        let target = self.duty_for(temp)?;
        Some(self.apply_response(target, ctx.dt_secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{Curve, CurveSet, EvalContext};
    use std::collections::HashMap;

    fn eval_at(c: &mut PointCurve, temp: Option<f64>) -> Option<f64> {
        let set = CurveSet::new();
        let sensors: HashMap<_, _> = [("t".to_string(), temp)].into();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        c.evaluate(&ctx)
    }

    fn curve() -> PointCurve {
        PointCurve::new("t".into(), vec![(30.0, 20.0), (70.0, 100.0), (50.0, 50.0)])
    }

    #[test]
    fn interpolates_linearly_between_points_regardless_of_input_order() {
        assert_eq!(eval_at(&mut curve(), Some(40.0)), Some(35.0));
        assert_eq!(eval_at(&mut curve(), Some(60.0)), Some(75.0));
    }

    #[test]
    fn clamps_below_first_and_above_last_point() {
        assert_eq!(eval_at(&mut curve(), Some(10.0)), Some(20.0));
        assert_eq!(eval_at(&mut curve(), Some(90.0)), Some(100.0));
    }

    #[test]
    fn exact_point_returns_its_duty() {
        assert_eq!(eval_at(&mut curve(), Some(50.0)), Some(50.0));
    }

    #[test]
    fn unavailable_sensor_returns_none() {
        assert_eq!(eval_at(&mut curve(), None), None);
    }

    #[test]
    fn empty_points_returns_none() {
        let mut c = PointCurve::new("t".into(), vec![]);
        assert_eq!(eval_at(&mut c, Some(50.0)), None);
    }

    #[test]
    fn hysteresis_ignores_small_moves_in_both_directions() {
        let mut c = curve().with_hysteresis(2.0, 5.0);
        assert_eq!(eval_at(&mut c, Some(40.0)), Some(35.0));
        assert_eq!(eval_at(&mut c, Some(41.9)), Some(35.0));
        assert_eq!(eval_at(&mut c, Some(36.0)), Some(35.0));
        let result = eval_at(&mut c, Some(42.1)).unwrap();
        assert!(
            (result - 38.15).abs() < 1e-9,
            "expected ~38.15, got {result}"
        );
    }

    #[test]
    fn hysteresis_tracks_after_large_drop() {
        let mut c = curve().with_hysteresis(2.0, 5.0);
        assert_eq!(eval_at(&mut c, Some(60.0)), Some(75.0));
        assert_eq!(eval_at(&mut c, Some(54.0)), Some(60.0));
    }

    #[test]
    fn hysteresis_survives_unavailable_reading() {
        let mut c = curve().with_hysteresis(2.0, 5.0);
        assert_eq!(eval_at(&mut c, Some(40.0)), Some(35.0));
        assert_eq!(eval_at(&mut c, None), None);
        assert_eq!(eval_at(&mut c, Some(41.0)), Some(35.0));
    }

    #[test]
    fn response_limits_rise_and_fall_per_second() {
        let mut c = curve().with_response(10.0, 20.0);
        assert_eq!(eval_at(&mut c, Some(30.0)), Some(20.0));
        assert_eq!(eval_at(&mut c, Some(70.0)), Some(30.0));
        assert_eq!(eval_at(&mut c, Some(70.0)), Some(40.0));
        assert_eq!(eval_at(&mut c, Some(30.0)), Some(20.0));
    }

    #[test]
    fn response_scales_with_dt() {
        let mut c = curve().with_response(10.0, 10.0);
        let set = CurveSet::new();
        let s1: HashMap<_, _> = [("t".to_string(), Some(30.0))].into();
        let ctx1 = EvalContext::new(&set, &s1, 1.0);
        assert_eq!(c.evaluate(&ctx1), Some(20.0));
        let s2: HashMap<_, _> = [("t".to_string(), Some(70.0))].into();
        let ctx2 = EvalContext::new(&set, &s2, 0.5);
        assert_eq!(c.evaluate(&ctx2), Some(25.0));
    }

    #[test]
    fn response_reaches_target_without_overshoot() {
        let mut c = curve().with_response(50.0, 50.0);
        assert_eq!(eval_at(&mut c, Some(30.0)), Some(20.0));
        assert_eq!(eval_at(&mut c, Some(50.0)), Some(50.0));
    }
}
