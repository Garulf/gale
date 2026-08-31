use crate::curve::{Curve, EvalContext};

pub struct FlatCurve {
    pub duty: f64,
}

impl Curve for FlatCurve {
    fn evaluate(&mut self, _ctx: &EvalContext) -> Option<f64> {
        Some(self.duty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{CurveSet, EvalContext};
    use std::collections::HashMap;

    #[test]
    fn flat_always_returns_its_duty() {
        let set = CurveSet::new();
        let sensors = HashMap::new();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        let mut c = FlatCurve { duty: 45.0 };
        assert_eq!(c.evaluate(&ctx), Some(45.0));
    }
}
