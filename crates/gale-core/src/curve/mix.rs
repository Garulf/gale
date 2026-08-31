use crate::curve::{Curve, EvalContext};
use crate::Id;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MixMode {
    Max,
    Min,
    Avg,
}

pub struct MixCurve {
    pub sources: Vec<Id>,
    pub mode: MixMode,
}

impl Curve for MixCurve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
        let values: Vec<f64> = self.sources.iter().filter_map(|id| ctx.resolve(id)).collect();
        if values.is_empty() {
            return None;
        }
        Some(match self.mode {
            MixMode::Max => values.iter().copied().fold(f64::MIN, f64::max),
            MixMode::Min => values.iter().copied().fold(f64::MAX, f64::min),
            MixMode::Avg => values.iter().sum::<f64>() / values.len() as f64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::flat::FlatCurve;
    use crate::curve::{Curve, CurveSet, EvalContext};
    use std::collections::HashMap;

    struct Unavailable;
    impl Curve for Unavailable {
        fn evaluate(&mut self, _ctx: &EvalContext) -> Option<f64> {
            None
        }
    }

    fn set() -> CurveSet {
        let mut s = CurveSet::new();
        s.insert("a".into(), Box::new(FlatCurve { duty: 20.0 }));
        s.insert("b".into(), Box::new(FlatCurve { duty: 60.0 }));
        s.insert("dead".into(), Box::new(Unavailable));
        s
    }

    fn eval(mode: MixMode, sources: &[&str]) -> Option<f64> {
        let set = set();
        let sensors = HashMap::new();
        let ctx = EvalContext::new(&set, &sensors, 1.0);
        let mut c = MixCurve {
            sources: sources.iter().map(|s| s.to_string()).collect(),
            mode,
        };
        c.evaluate(&ctx)
    }

    #[test]
    fn max_min_avg_combine_available_sources() {
        assert_eq!(eval(MixMode::Max, &["a", "b"]), Some(60.0));
        assert_eq!(eval(MixMode::Min, &["a", "b"]), Some(20.0));
        assert_eq!(eval(MixMode::Avg, &["a", "b"]), Some(40.0));
    }

    #[test]
    fn unavailable_sources_are_skipped_not_zero() {
        assert_eq!(eval(MixMode::Avg, &["a", "b", "dead"]), Some(40.0));
        assert_eq!(eval(MixMode::Max, &["dead"]), None);
        assert_eq!(eval(MixMode::Max, &[]), None);
    }
}
