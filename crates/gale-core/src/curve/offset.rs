use crate::curve::{Curve, EvalContext};
use crate::Id;

pub struct OffsetCurve {
    pub source: Id,
    pub add: f64,
    pub scale: f64,
}

impl Curve for OffsetCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        let duty = ctx.resolve(&self.source)?;
        Some((duty * self.scale + self.add).clamp(0.0, 100.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::flat::FlatCurve;
    use crate::curve::{CurveSet, EvalContext};
    use std::collections::HashMap;

    fn eval(source_duty: f64, add: f64, scale: f64) -> Option<f64> {
        let mut set = CurveSet::new();
        set.insert("base".into(), Box::new(FlatCurve { duty: source_duty }));
        let sensors = HashMap::new();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        let mut c = OffsetCurve {
            source: "base".into(),
            add,
            scale,
        };
        c.evaluate(&ctx)
    }

    #[test]
    fn scales_then_adds_and_clamps_to_the_duty_range() {
        assert_eq!(eval(40.0, 10.0, 1.0), Some(50.0));
        assert_eq!(eval(40.0, 0.0, 2.0), Some(80.0));
        assert_eq!(eval(40.0, 0.0, 3.0), Some(100.0));
        assert_eq!(eval(40.0, -50.0, 1.0), Some(0.0));
    }

    #[test]
    fn missing_source_yields_none() {
        let set = CurveSet::new();
        let sensors = HashMap::new();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        let mut c = OffsetCurve {
            source: "nope".into(),
            add: 0.0,
            scale: 1.0,
        };
        assert_eq!(c.evaluate(&ctx), None);
    }
}
