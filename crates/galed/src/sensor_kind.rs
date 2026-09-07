use gale_core::config::ProfileConfig;
use gale_hw::{Id, SensorKind};
use std::collections::HashMap;

const MAX_DEPTH: usize = 16;

fn resolve(
    id: &str,
    profile: &ProfileConfig,
    known: &HashMap<Id, SensorKind>,
    depth: usize,
) -> Option<SensorKind> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Some(name) = id.strip_prefix("virtual/") {
        let inputs = profile.sensors.get(name)?.inputs();
        return resolve(inputs.first()?, profile, known, depth + 1);
    }
    known.get(id).copied()
}

pub fn sensor_kind_of(
    id: &str,
    profile: &ProfileConfig,
    known: &HashMap<Id, SensorKind>,
) -> Option<SensorKind> {
    resolve(id, profile, known, 0)
}

pub fn virtual_sensor_kind(
    profile: &ProfileConfig,
    name: &str,
    known: &HashMap<Id, SensorKind>,
) -> SensorKind {
    profile
        .sensors
        .get(name)
        .and_then(|cfg| resolve(cfg.inputs().first()?, profile, known, 1))
        .unwrap_or(SensorKind::Temp)
}
