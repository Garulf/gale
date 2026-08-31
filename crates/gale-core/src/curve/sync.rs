use crate::curve::{Curve, EvalContext};
use crate::Id;

pub struct SyncCurve {
    pub source: Id,
}

impl Curve for SyncCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        ctx.resolve(&self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::flat::FlatCurve;
    use crate::curve::{CurveSet, EvalContext};
    use std::collections::HashMap;

    #[test]
    fn sync_mirrors_source_and_none_when_source_missing() {
        let mut set = CurveSet::new();
        set.insert("src".into(), Box::new(FlatCurve { duty: 33.0 }));
        let sensors = HashMap::new();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        let mut c = SyncCurve { source: "src".into() };
        assert_eq!(c.evaluate(&ctx), Some(33.0));
        let mut miss = SyncCurve { source: "ghost".into() };
        assert_eq!(miss.evaluate(&ctx), None);
    }
}
