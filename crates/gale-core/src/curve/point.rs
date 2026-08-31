use crate::curve::{Curve, EvalContext};
use crate::Id;

pub struct PointCurve {
    sensor: Id,
    points: Vec<(f64, f64)>,
}

impl PointCurve {
    pub fn new(sensor: Id, mut points: Vec<(f64, f64)>) -> Self {
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self { sensor, points }
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
        let temp = ctx.sensor(&self.sensor)?;
        self.duty_for(temp)
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
}
