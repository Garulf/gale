use crate::config::{virtual_id, ConfigError, ProfileConfig, VirtualSensorConfig};
use crate::Id;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, PartialEq)]
struct Sample {
    t: f64,
    value: f64,
    dt: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    window_s: f64,
    now: f64,
    samples: VecDeque<Sample>,
}

impl Window {
    pub fn new(window_s: f64) -> Self {
        Self {
            window_s,
            now: 0.0,
            samples: VecDeque::new(),
        }
    }

    pub fn advance(&mut self, dt_secs: f64) {
        self.now += dt_secs;
        let threshold = self.now - self.window_s;
        while let Some(front) = self.samples.front() {
            if front.t <= threshold {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn push(&mut self, value: f64, dt_secs: f64) {
        self.samples.push_back(Sample {
            t: self.now,
            value,
            dt: dt_secs,
        });
    }

    pub fn time_weighted_mean(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let mut weighted = 0.0;
        let mut total_dt = 0.0;
        for sample in &self.samples {
            weighted += sample.value * sample.dt;
            total_dt += sample.dt;
        }
        if total_dt == 0.0 {
            None
        } else {
            Some(weighted / total_dt)
        }
    }

    pub fn slope_per_minute(&self) -> Option<f64> {
        if self.samples.len() < 2 {
            return None;
        }
        let oldest = self.samples.front().expect("len checked above");
        let newest = self.samples.back().expect("len checked above");
        let span = newest.t - oldest.t;
        if span == 0.0 {
            None
        } else {
            Some((newest.value - oldest.value) * 60.0 / span)
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[derive(Debug)]
enum NodeKind {
    Max(Vec<Id>),
    Min(Vec<Id>),
    Mean {
        inputs: Vec<Id>,
        window: Option<Window>,
    },
    Sum(Vec<Id>),
    Subtract(Vec<Id>),
    Offset {
        input: Id,
        add: f64,
        scale: f64,
    },
    Delta {
        input: Id,
        window: Window,
    },
    Webhook,
}

#[derive(Debug)]
struct Node {
    id: Id,
    kind: NodeKind,
}

#[derive(Debug, Default)]
pub struct VirtualSensors {
    nodes: Vec<Node>,
}

impl VirtualSensors {
    pub fn build(profile: &ProfileConfig) -> Result<Self, ConfigError> {
        let order = profile.validate_virtual_sensors()?;
        let mut nodes = Vec::with_capacity(order.len());
        for name in order {
            let cfg = profile
                .sensors
                .get(&name)
                .expect("validated name exists in sensors");
            let kind = match cfg {
                VirtualSensorConfig::Max { inputs } => NodeKind::Max(inputs.clone()),
                VirtualSensorConfig::Min { inputs } => NodeKind::Min(inputs.clone()),
                VirtualSensorConfig::Mean { inputs, window_s } => NodeKind::Mean {
                    inputs: inputs.clone(),
                    window: window_s.map(Window::new),
                },
                VirtualSensorConfig::Sum { inputs } => NodeKind::Sum(inputs.clone()),
                VirtualSensorConfig::Subtract { inputs } => NodeKind::Subtract(inputs.clone()),
                VirtualSensorConfig::Offset { input, add, scale } => NodeKind::Offset {
                    input: input.clone(),
                    add: *add,
                    scale: *scale,
                },
                VirtualSensorConfig::Delta { input, window_s } => NodeKind::Delta {
                    input: input.clone(),
                    window: Window::new(*window_s),
                },
                VirtualSensorConfig::Webhook { .. } => NodeKind::Webhook,
            };
            nodes.push(Node {
                id: virtual_id(&name),
                kind,
            });
        }
        Ok(Self { nodes })
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn ids(&self) -> Vec<Id> {
        self.nodes.iter().map(|node| node.id.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn evaluate(&mut self, sensors: &mut HashMap<Id, Option<f64>>, dt_secs: f64) {
        for node in &mut self.nodes {
            let output = match &mut node.kind {
                NodeKind::Max(inputs) => {
                    fold_available(inputs.iter().map(|id| read(sensors, id)), f64::max)
                }
                NodeKind::Min(inputs) => {
                    fold_available(inputs.iter().map(|id| read(sensors, id)), f64::min)
                }
                NodeKind::Mean {
                    inputs,
                    window: None,
                } => mean_available(inputs.iter().map(|id| read(sensors, id))),
                NodeKind::Mean {
                    inputs,
                    window: Some(window),
                } => {
                    window.advance(dt_secs);
                    match mean_available(inputs.iter().map(|id| read(sensors, id))) {
                        Some(value) => {
                            window.push(value, dt_secs);
                            window.time_weighted_mean()
                        }
                        None => None,
                    }
                }
                NodeKind::Sum(inputs) => {
                    fold_available(inputs.iter().map(|id| read(sensors, id)), |a, b| a + b)
                }
                NodeKind::Subtract(inputs) => subtract_available(
                    inputs.first().and_then(|id| read(sensors, id)),
                    inputs.iter().skip(1).map(|id| read(sensors, id)),
                ),
                NodeKind::Offset { input, add, scale } => {
                    read(sensors, input).map(|v| v * *scale + *add)
                }
                NodeKind::Delta { input, window } => {
                    window.advance(dt_secs);
                    match read(sensors, input) {
                        Some(value) => {
                            window.push(value, dt_secs);
                            window.slope_per_minute()
                        }
                        None => None,
                    }
                }
                NodeKind::Webhook => read(sensors, &node.id),
            };
            sensors.insert(node.id.clone(), output.and_then(finite));
        }
    }
}

fn read(sensors: &HashMap<Id, Option<f64>>, id: &Id) -> Option<f64> {
    sensors.get(id).copied().flatten()
}

pub fn fold_available(
    values: impl Iterator<Item = Option<f64>>,
    pick: fn(f64, f64) -> f64,
) -> Option<f64> {
    values.flatten().reduce(pick)
}

pub fn subtract_available(
    first: Option<f64>,
    rest: impl Iterator<Item = Option<f64>>,
) -> Option<f64> {
    let first = first?;
    Some(first - rest.flatten().sum::<f64>())
}

pub fn mean_available(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let mut sum = 0.0;
    let mut count = 0usize;
    for value in values.flatten() {
        sum += value;
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

fn finite(value: f64) -> Option<f64> {
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::virtual_id;

    fn sensors(pairs: &[(&str, Option<f64>)]) -> HashMap<Id, Option<f64>> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn parse_profile(toml_src: &str) -> ProfileConfig {
        toml::from_str(toml_src).expect("valid profile toml")
    }

    #[test]
    fn max_and_min_follow_the_availability_rule() {
        let profile = parse_profile(
            r#"
            [sensors.cpu_hot]
            type = "max"
            inputs = ["a", "b"]

            [sensors.cpu_cold]
            type = "min"
            inputs = ["a", "b"]
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("a", Some(60.0)), ("b", Some(72.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("cpu_hot")], Some(72.0));
        assert_eq!(map[&virtual_id("cpu_cold")], Some(60.0));

        let mut map = sensors(&[("a", Some(60.0)), ("b", None)]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("cpu_hot")], Some(60.0));
        assert_eq!(map[&virtual_id("cpu_cold")], Some(60.0));

        let mut map = sensors(&[("a", None), ("b", None)]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("cpu_hot")], None);
        assert_eq!(map[&virtual_id("cpu_cold")], None);

        let mut map = sensors(&[("a", Some(60.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("cpu_hot")], Some(60.0));
        assert_eq!(map[&virtual_id("cpu_cold")], Some(60.0));
    }

    #[test]
    fn sum_and_subtract_follow_the_availability_rule() {
        let profile = parse_profile(
            r#"
            [sensors.total]
            type = "sum"
            inputs = ["a", "b"]

            [sensors.gap]
            type = "subtract"
            inputs = ["b", "a"]
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("a", Some(60.0)), ("b", Some(72.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("total")], Some(132.0));
        assert_eq!(map[&virtual_id("gap")], Some(12.0));

        let mut map = sensors(&[("a", None), ("b", Some(72.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("total")], Some(72.0));
        assert_eq!(map[&virtual_id("gap")], Some(72.0));

        let mut map = sensors(&[("a", Some(60.0)), ("b", None)]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("gap")], None);
    }

    #[test]
    fn mean_without_window_averages_available_inputs() {
        let profile = parse_profile(
            r#"
            [sensors.m]
            type = "mean"
            inputs = ["a", "b", "c"]
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("a", Some(40.0)), ("b", None), ("c", Some(50.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("m")], Some(45.0));

        let mut map = sensors(&[("a", None), ("b", None), ("c", None)]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("m")], None);
    }

    #[test]
    fn offset_scales_then_adds() {
        let profile = parse_profile(
            r#"
            [sensors.o]
            type = "offset"
            input = "t"
            add = -5.0
            scale = 1.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let mut map = sensors(&[("t", Some(70.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("o")], Some(65.0));

        let profile = parse_profile(
            r#"
            [sensors.o]
            type = "offset"
            input = "t"
            add = 32.0
            scale = 1.8
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let mut map = sensors(&[("t", Some(100.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("o")], Some(212.0));

        let mut map = sensors(&[("t", None)]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("o")], None);
    }

    #[test]
    fn offset_maps_non_finite_results_to_none() {
        let profile = parse_profile(
            r#"
            [sensors.o]
            type = "offset"
            input = "t"
            add = 0.0
            scale = 1.0e308
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let mut map = sensors(&[("t", Some(1.0e10))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("o")], None);
    }

    #[test]
    fn max_maps_non_finite_results_to_none() {
        let profile = parse_profile(
            r#"
            [sensors.m]
            type = "max"
            inputs = ["a", "b"]
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let mut map = sensors(&[("a", Some(f64::INFINITY)), ("b", Some(10.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("m")], None);
    }

    #[test]
    fn min_maps_non_finite_results_to_none() {
        let profile = parse_profile(
            r#"
            [sensors.m]
            type = "min"
            inputs = ["a", "b"]
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let mut map = sensors(&[("a", Some(f64::NEG_INFINITY)), ("b", Some(10.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("m")], None);
    }

    #[test]
    fn mean_maps_non_finite_results_to_none() {
        let profile = parse_profile(
            r#"
            [sensors.m]
            type = "mean"
            inputs = ["a"]
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let mut map = sensors(&[("a", Some(f64::NAN))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("m")], None);
    }

    #[test]
    fn delta_maps_non_finite_results_to_none() {
        let profile = parse_profile(
            r#"
            [sensors.d]
            type = "delta"
            input = "t"
            window_s = 60.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("t", Some(0.0))]);
        vs.evaluate(&mut map, 10.0);

        let mut map = sensors(&[("t", Some(f64::INFINITY))]);
        vs.evaluate(&mut map, 10.0);
        assert_eq!(map[&virtual_id("d")], None);
    }

    #[test]
    fn windowed_mean_matches_scripted_series() {
        let profile = parse_profile(
            r#"
            [sensors.s]
            type = "mean"
            inputs = ["t"]
            window_s = 3.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let series = [10.0, 20.0, 30.0, 40.0, 50.0];
        let expected = [Some(10.0), Some(15.0), Some(20.0), Some(30.0), Some(40.0)];
        for (t, exp) in series.iter().zip(expected.iter()) {
            let mut map = sensors(&[("t", Some(*t))]);
            vs.evaluate(&mut map, 1.0);
            assert_eq!(map[&virtual_id("s")], *exp);
        }
    }

    #[test]
    fn windowed_mean_weights_by_dt() {
        let profile = parse_profile(
            r#"
            [sensors.s]
            type = "mean"
            inputs = ["t"]
            window_s = 10.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("t", Some(10.0))]);
        vs.evaluate(&mut map, 1.0);

        let mut map = sensors(&[("t", Some(40.0))]);
        vs.evaluate(&mut map, 3.0);
        assert_eq!(map[&virtual_id("s")], Some(32.5));
    }

    #[test]
    fn windowed_mean_skips_none_ticks_without_resetting() {
        let profile = parse_profile(
            r#"
            [sensors.s]
            type = "mean"
            inputs = ["t"]
            window_s = 3.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("t", Some(10.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("s")], Some(10.0));

        let mut map = sensors(&[("t", None)]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("s")], None);

        let mut map = sensors(&[("t", Some(30.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("s")], Some(20.0));
    }

    #[test]
    fn delta_matches_scripted_series() {
        let profile = parse_profile(
            r#"
            [sensors.d]
            type = "delta"
            input = "t"
            window_s = 60.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        let series = [40.0, 41.0, 43.0, 46.0, 40.0, 34.0, 34.0];
        let expected = [
            None,
            Some(6.0),
            Some(9.0),
            Some(12.0),
            Some(0.0),
            Some(-7.2),
            Some(-8.4),
        ];
        for (t, exp) in series.iter().zip(expected.iter()) {
            let mut map = sensors(&[("t", Some(*t))]);
            vs.evaluate(&mut map, 10.0);
            assert_eq!(map[&virtual_id("d")], *exp);
        }
    }

    #[test]
    fn delta_is_none_on_a_none_tick_and_resumes() {
        let profile = parse_profile(
            r#"
            [sensors.d]
            type = "delta"
            input = "t"
            window_s = 60.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("t", Some(40.0))]);
        vs.evaluate(&mut map, 10.0);
        assert_eq!(map[&virtual_id("d")], None);

        let mut map = sensors(&[("t", None)]);
        vs.evaluate(&mut map, 10.0);
        assert_eq!(map[&virtual_id("d")], None);

        let mut map = sensors(&[("t", Some(46.0))]);
        vs.evaluate(&mut map, 10.0);
        assert_eq!(map[&virtual_id("d")], Some(18.0));
    }

    #[test]
    fn chain_evaluates_in_dependency_order_regardless_of_name_order() {
        let profile = parse_profile(
            r#"
            [sensors.a]
            type = "max"
            inputs = ["virtual/b"]

            [sensors.b]
            type = "offset"
            input = "hw/t"
            add = 1.0
            scale = 1.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();
        assert_eq!(vs.ids(), vec![virtual_id("b"), virtual_id("a")]);

        let mut map = sensors(&[("hw/t", Some(50.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("a")], Some(51.0));
        assert_eq!(map[&virtual_id("b")], Some(51.0));
    }

    #[test]
    fn build_rejects_cycles_with_invalid() {
        let profile = parse_profile(
            r#"
            [sensors.a]
            type = "max"
            inputs = ["virtual/b"]

            [sensors.b]
            type = "min"
            inputs = ["virtual/a"]
            "#,
        );
        let err = VirtualSensors::build(&profile).unwrap_err();
        match err {
            ConfigError::Invalid(msg) => assert!(msg.contains("cycle")),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn rebuild_resets_window_state() {
        let profile = parse_profile(
            r#"
            [sensors.s]
            type = "mean"
            inputs = ["t"]
            window_s = 3.0
            "#,
        );
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("t", Some(10.0))]);
        vs.evaluate(&mut map, 1.0);

        let mut map = sensors(&[("t", Some(20.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("s")], Some(15.0));

        let mut vs = VirtualSensors::build(&profile).unwrap();
        let mut map = sensors(&[("t", Some(30.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("s")], Some(30.0));
    }

    #[test]
    fn empty_set_leaves_map_untouched() {
        let mut vs = VirtualSensors::empty();
        let mut map = sensors(&[("a", Some(1.0))]);
        let before = map.clone();
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map, before);
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn webhook_passes_through_a_seeded_value() {
        let profile = parse_profile(&format!(
            r#"
            [sensors.hook]
            type = "webhook"
            token = "{TOKEN}"
            "#,
        ));
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("virtual/hook", Some(41.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("hook")], Some(41.0));
    }

    #[test]
    fn webhook_with_nothing_seeded_evaluates_to_none() {
        let profile = parse_profile(&format!(
            r#"
            [sensors.hook]
            type = "webhook"
            token = "{TOKEN}"
            "#,
        ));
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map.get(&virtual_id("hook")), Some(&None));
    }

    #[test]
    fn webhook_seeded_non_finite_becomes_none() {
        let profile = parse_profile(&format!(
            r#"
            [sensors.hook]
            type = "webhook"
            token = "{TOKEN}"
            "#,
        ));
        let mut vs = VirtualSensors::build(&profile).unwrap();

        let mut map = sensors(&[("virtual/hook", Some(f64::NAN))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("hook")], None);
    }

    #[test]
    fn webhook_feeds_downstream_max() {
        let profile = parse_profile(&format!(
            r#"
            [sensors.hook]
            type = "webhook"
            token = "{TOKEN}"

            [sensors.agg]
            type = "max"
            inputs = ["virtual/hook", "t"]
            "#,
        ));
        let mut vs = VirtualSensors::build(&profile).unwrap();
        assert_eq!(vs.ids(), vec![virtual_id("hook"), virtual_id("agg")]);

        let mut map = sensors(&[("virtual/hook", Some(55.0)), ("t", Some(50.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("agg")], Some(55.0));

        let mut map = sensors(&[("t", Some(50.0))]);
        vs.evaluate(&mut map, 1.0);
        assert_eq!(map[&virtual_id("agg")], Some(50.0));
        assert_eq!(map[&virtual_id("hook")], None);
    }

    #[test]
    fn virtual_sensors_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<VirtualSensors>();
    }
}
