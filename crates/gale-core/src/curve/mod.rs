pub mod flat;
pub mod mix;
pub mod point;
pub mod sync;

use crate::Id;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

pub trait Curve {
    fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64>;
}

#[derive(Default)]
pub struct CurveSet {
    curves: HashMap<Id, RefCell<Box<dyn Curve>>>,
}

impl CurveSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: Id, curve: Box<dyn Curve>) {
        self.curves.insert(id, RefCell::new(curve));
    }

    pub fn contains(&self, id: &str) -> bool {
        self.curves.contains_key(id)
    }
}

pub struct EvalContext<'a> {
    pub dt_secs: f64,
    set: &'a CurveSet,
    sensors: &'a HashMap<Id, Option<f64>>,
    visiting: RefCell<HashSet<Id>>,
    memo: RefCell<HashMap<Id, Option<f64>>>,
}

impl<'a> EvalContext<'a> {
    pub fn new(set: &'a CurveSet, sensors: &'a HashMap<Id, Option<f64>>, dt_secs: f64) -> Self {
        Self {
            dt_secs,
            set,
            sensors,
            visiting: RefCell::new(HashSet::new()),
            memo: RefCell::new(HashMap::new()),
        }
    }

    pub fn sensor(&self, id: &str) -> Option<f64> {
        self.sensors.get(id).copied().flatten()
    }

    pub fn resolve(&self, id: &str) -> Option<f64> {
        if let Some(cached) = self.memo.borrow().get(id) {
            return *cached;
        }
        if !self.visiting.borrow_mut().insert(id.to_string()) {
            return None;
        }
        let result = match self.set.curves.get(id) {
            Some(cell) => cell.borrow_mut().evaluate(self),
            None => None,
        };
        self.visiting.borrow_mut().remove(id);
        self.memo.borrow_mut().insert(id.to_string(), result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Const(Option<f64>);
    impl Curve for Const {
        fn evaluate(&mut self, _ctx: &EvalContext) -> Option<f64> {
            self.0
        }
    }

    struct Follows(Id);
    impl Curve for Follows {
        fn evaluate(&mut self, ctx: &EvalContext) -> Option<f64> {
            ctx.resolve(&self.0)
        }
    }

    struct Counting {
        calls: std::rc::Rc<std::cell::Cell<u32>>,
    }
    impl Curve for Counting {
        fn evaluate(&mut self, _ctx: &EvalContext) -> Option<f64> {
            self.calls.set(self.calls.get() + 1);
            Some(42.0)
        }
    }

    fn sensors(pairs: &[(&str, Option<f64>)]) -> HashMap<Id, Option<f64>> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn sensor_returns_value_and_none_for_missing_or_stale() {
        let set = CurveSet::new();
        let s = sensors(&[("t1", Some(50.0)), ("t2", None)]);
        let ctx = EvalContext::new(&set, &s, 1.0);
        assert_eq!(ctx.sensor("t1"), Some(50.0));
        assert_eq!(ctx.sensor("t2"), None);
        assert_eq!(ctx.sensor("missing"), None);
    }

    #[test]
    fn resolve_evaluates_known_curve_and_none_for_unknown() {
        let mut set = CurveSet::new();
        set.insert("a".into(), Box::new(Const(Some(30.0))));
        let s = sensors(&[]);
        let ctx = EvalContext::new(&set, &s, 1.0);
        assert_eq!(ctx.resolve("a"), Some(30.0));
        assert_eq!(ctx.resolve("nope"), None);
    }

    #[test]
    fn resolve_memoizes_within_one_context() {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut set = CurveSet::new();
        set.insert("a".into(), Box::new(Counting { calls: calls.clone() }));
        let s = sensors(&[]);
        let ctx = EvalContext::new(&set, &s, 1.0);
        assert_eq!(ctx.resolve("a"), Some(42.0));
        assert_eq!(ctx.resolve("a"), Some(42.0));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn resolve_breaks_cycles_with_none() {
        let mut set = CurveSet::new();
        set.insert("a".into(), Box::new(Follows("b".into())));
        set.insert("b".into(), Box::new(Follows("a".into())));
        let s = sensors(&[]);
        let ctx = EvalContext::new(&set, &s, 1.0);
        assert_eq!(ctx.resolve("a"), None);
    }
}
