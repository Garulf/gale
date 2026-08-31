use crate::backend_handle::BackendHandle;
use gale_core::build::build_engine;
use gale_core::config::{ConfigError, GaleConfig};
use gale_core::engine::FanEngine;
use gale_hw::{HwError, Id};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Snapshot {
    pub sensors: HashMap<Id, Option<f64>>,
    pub duties: HashMap<Id, f64>,
    pub manual: HashMap<Id, f64>,
    pub active_profile: String,
}

struct HostState {
    config: GaleConfig,
    engine: FanEngine,
    manual: HashMap<Id, f64>,
}

pub struct EngineHost {
    state: Mutex<HostState>,
    claimed: Mutex<HashSet<Id>>,
    backend: BackendHandle,
    snapshot_tx: watch::Sender<Snapshot>,
}

impl EngineHost {
    pub fn new(config: GaleConfig, backend: BackendHandle) -> Result<Arc<Self>, ConfigError> {
        let engine = build_engine(&config)?;
        let (snapshot_tx, _) = watch::channel(Snapshot::default());
        Ok(Arc::new(Self {
            state: Mutex::new(HostState {
                config,
                engine,
                manual: HashMap::new(),
            }),
            claimed: Mutex::new(HashSet::new()),
            backend,
            snapshot_tx,
        }))
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.snapshot_tx.subscribe()
    }

    pub fn config(&self) -> GaleConfig {
        self.state.lock().unwrap().config.clone()
    }

    pub async fn tick(&self, dt_secs: f64) {
        let sensors = self.backend.read_all().await;
        let (mut duties, manual, active_profile, assigned) = {
            let mut state = self.state.lock().unwrap();
            let mut duties = state.engine.tick(&sensors, dt_secs);
            for (id, duty) in &state.manual {
                duties.insert(id.clone(), *duty);
            }
            let assigned: HashSet<Id> = state.engine.assignments().keys().cloned().collect();
            (
                duties,
                state.manual.clone(),
                state.config.active_profile.clone(),
                assigned,
            )
        };
        let mut failed: HashSet<Id> = HashSet::new();
        for (id, duty) in &duties {
            match self.backend.set_duty(id, *duty).await {
                Ok(()) => {
                    self.claimed.lock().unwrap().insert(id.clone());
                }
                Err(error) => {
                    tracing::warn!(%id, %error, "duty write failed, skipping control this tick");
                    failed.insert(id.clone());
                }
            }
        }
        duties.retain(|id, _| !failed.contains(id));

        let stale: Vec<Id> = {
            let claimed = self.claimed.lock().unwrap();
            claimed
                .iter()
                .filter(|id| !assigned.contains(*id) && !manual.contains_key(*id))
                .cloned()
                .collect()
        };
        for id in stale {
            if self.backend.release(&id).await.is_ok() {
                self.claimed.lock().unwrap().remove(&id);
            }
        }

        self.snapshot_tx.send_replace(Snapshot {
            sensors,
            duties,
            manual,
            active_profile,
        });
    }

    pub async fn replace_config(&self, new: GaleConfig) -> Result<(), ConfigError> {
        let engine = build_engine(&new)?;
        let assigned: HashSet<Id> = engine.assignments().keys().cloned().collect();
        let stale: Vec<Id> = {
            let mut state = self.state.lock().unwrap();
            state.engine = engine;
            state.config = new;
            let claimed = self.claimed.lock().unwrap();
            claimed
                .iter()
                .filter(|id| !assigned.contains(*id) && !state.manual.contains_key(*id))
                .cloned()
                .collect()
        };
        for id in stale {
            if self.backend.release(&id).await.is_ok() {
                self.claimed.lock().unwrap().remove(&id);
            }
        }
        Ok(())
    }

    pub async fn set_manual(&self, id: &str, duty: f64) -> Result<(), HwError> {
        self.backend.set_duty(id, duty).await?;
        self.claimed.lock().unwrap().insert(id.to_string());
        self.state
            .lock()
            .unwrap()
            .manual
            .insert(id.to_string(), duty);
        Ok(())
    }

    pub async fn clear_manual(&self, id: &str) -> Result<(), HwError> {
        let assigned = {
            let mut state = self.state.lock().unwrap();
            state.manual.remove(id);
            state.engine.assignments().contains_key(id)
        };
        if !assigned {
            self.backend.release(id).await?;
            self.claimed.lock().unwrap().remove(id);
        }
        Ok(())
    }

    pub fn claimed_ids(&self) -> Vec<Id> {
        self.claimed.lock().unwrap().iter().cloned().collect()
    }

    pub fn blocking_release_claimed(&self) {
        for id in self.claimed_ids() {
            self.backend.blocking_release(&id);
        }
    }

    pub async fn release_all(&self) {
        let ids: Vec<Id> = self.claimed.lock().unwrap().drain().collect();
        for id in ids {
            if let Err(error) = self.backend.release(&id).await {
                tracing::warn!(%id, %error, "release failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend_handle::BackendHandle;
    use crate::test_support::{Recorded, RecordingBackend};
    use gale_core::config::GaleConfig;
    use std::sync::{Arc, Mutex};

    const CONFIG: &str = r#"
active_profile = "p"

[profiles.p.curves.cpu]
type = "point"
sensor = "t1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.assignments]
"pwm1" = "cpu"
"pwm2" = "cpu"
"#;

    fn setup(sensors: &[(&str, Option<f64>)]) -> (Arc<EngineHost>, Arc<Mutex<Recorded>>) {
        let state = Arc::new(Mutex::new(Recorded {
            sensors: sensors.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            ..Recorded::default()
        }));
        let handle = BackendHandle::spawn(Box::new(RecordingBackend {
            state: state.clone(),
        }));
        let host = EngineHost::new(GaleConfig::from_toml(CONFIG).unwrap(), handle).unwrap();
        (host, state)
    }

    #[tokio::test]
    async fn tick_applies_curve_duties_and_publishes_snapshot() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        assert_eq!(state.lock().unwrap().duties["pwm1"], 60.0);
        assert_eq!(state.lock().unwrap().duties["pwm2"], 60.0);
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["t1"], Some(50.0));
        assert_eq!(snapshot.duties["pwm1"], 60.0);
        assert_eq!(snapshot.active_profile, "p");
    }

    #[tokio::test]
    async fn manual_override_beats_curve_and_write_errors_are_skipped() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        state.lock().unwrap().fail_ids.push("pwm2".to_string());
        host.set_manual("pwm1", 33.0).await.unwrap();
        host.tick(1.0).await;
        assert_eq!(state.lock().unwrap().duties["pwm1"], 33.0);
        assert!(!state.lock().unwrap().duties.contains_key("pwm2"));
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.manual["pwm1"], 33.0);
        assert!(!snapshot.duties.contains_key("pwm2"));
        assert_eq!(snapshot.duties["pwm1"], 33.0);
    }

    #[tokio::test]
    async fn clear_manual_release_is_not_reclaimed_by_next_tick() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.set_manual("extra/pwm", 50.0).await.unwrap();
        host.tick(1.0).await;
        host.clear_manual("extra/pwm").await.unwrap();
        assert_eq!(
            state.lock().unwrap().released,
            vec!["extra/pwm".to_string()]
        );
        host.tick(1.0).await;
        assert_eq!(
            state.lock().unwrap().released,
            vec!["extra/pwm".to_string()]
        );
    }

    #[tokio::test]
    async fn stale_claim_from_replaced_config_is_released_exactly_once() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        let mut new = GaleConfig::from_toml(CONFIG).unwrap();
        new.profiles
            .get_mut("p")
            .unwrap()
            .assignments
            .remove("pwm2");
        host.replace_config(new).await.unwrap();
        assert_eq!(state.lock().unwrap().released, vec!["pwm2".to_string()]);
        host.tick(1.0).await;
        host.tick(1.0).await;
        assert_eq!(state.lock().unwrap().released, vec!["pwm2".to_string()]);
    }

    #[tokio::test]
    async fn invalid_replacement_config_keeps_old_engine_running() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        let mut bad = GaleConfig::from_toml(CONFIG).unwrap();
        bad.active_profile = "ghost".into();
        assert!(host.replace_config(bad).await.is_err());
        host.tick(1.0).await;
        assert_eq!(state.lock().unwrap().duties["pwm1"], 60.0);
    }

    #[tokio::test]
    async fn replacement_releases_controls_dropped_from_new_config() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        let mut new = GaleConfig::from_toml(CONFIG).unwrap();
        new.profiles
            .get_mut("p")
            .unwrap()
            .assignments
            .remove("pwm2");
        host.replace_config(new).await.unwrap();
        assert_eq!(state.lock().unwrap().released, vec!["pwm2".to_string()]);
    }

    #[tokio::test]
    async fn clear_manual_on_unassigned_control_releases_it() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.set_manual("extra/pwm", 80.0).await.unwrap();
        host.clear_manual("extra/pwm").await.unwrap();
        assert_eq!(
            state.lock().unwrap().released,
            vec!["extra/pwm".to_string()]
        );
        host.set_manual("pwm1", 20.0).await.unwrap();
        host.clear_manual("pwm1").await.unwrap();
        assert_eq!(state.lock().unwrap().released.len(), 1);
    }

    #[tokio::test]
    async fn claimed_ids_returns_claimed_set_after_tick() {
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        let mut ids = host.claimed_ids();
        ids.sort();
        assert_eq!(ids, vec!["pwm1".to_string(), "pwm2".to_string()]);
    }

    #[tokio::test]
    async fn release_all_releases_every_claimed_control_once() {
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        host.release_all().await;
        let mut released = state.lock().unwrap().released.clone();
        released.sort();
        assert_eq!(released, vec!["pwm1".to_string(), "pwm2".to_string()]);
        host.release_all().await;
        assert_eq!(state.lock().unwrap().released.len(), 2);
    }
}
