use crate::backend_pool::BackendPool;
use crate::claims_journal::{self, JournalEntry};
use crate::webhooks::WebhookStore;
use gale_core::build::build_engine;
use gale_core::config::{ConfigError, GaleConfig, ProfileConfig, VirtualSensorConfig};
use gale_core::engine::FanEngine;
use gale_hw::{HwError, Id};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tokio::sync::watch;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Snapshot {
    pub sensors: HashMap<Id, Option<f64>>,
    pub duties: HashMap<Id, f64>,
    pub curves: HashMap<Id, f64>,
    pub manual: HashMap<Id, f64>,
    pub overrides: HashSet<Id>,
    pub active_profile: String,
}

struct HostState {
    config: GaleConfig,
    engine: FanEngine,
    manual: HashMap<Id, f64>,
    webhooks: WebhookStore,
}

pub struct EngineHost {
    state: Mutex<HostState>,
    claimed: Mutex<HashSet<Id>>,
    backend: BackendPool,
    snapshot_tx: watch::Sender<Snapshot>,
    known_sensors: RwLock<HashSet<Id>>,
    platform_warnings: RwLock<Vec<String>>,
}

impl EngineHost {
    pub fn new(config: GaleConfig, backend: BackendPool) -> Result<Arc<Self>, ConfigError> {
        let engine = build_engine(&config)?;
        let webhooks = WebhookStore::from_config(&config);
        let (snapshot_tx, _) = watch::channel(Snapshot::default());
        Ok(Arc::new(Self {
            state: Mutex::new(HostState {
                config,
                engine,
                manual: HashMap::new(),
                webhooks,
            }),
            claimed: Mutex::new(HashSet::new()),
            backend,
            snapshot_tx,
            known_sensors: RwLock::new(HashSet::new()),
            platform_warnings: RwLock::new(Vec::new()),
        }))
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.snapshot_tx.subscribe()
    }

    pub fn config(&self) -> GaleConfig {
        self.state.lock().unwrap().config.clone()
    }

    pub fn set_known_sensors(&self, sensors: HashSet<Id>) {
        *self.known_sensors.write().unwrap() = sensors;
    }

    pub fn set_platform_warnings(&self, warnings: Vec<String>) {
        *self.platform_warnings.write().unwrap() = warnings;
    }

    pub fn warnings(&self, config: &GaleConfig) -> Vec<String> {
        let mut warnings = self.platform_warnings.read().unwrap().clone();
        warnings.extend(self.config_warnings(config));
        warnings
    }

    pub fn log_config_warnings(&self, config: &GaleConfig) {
        for warning in self.config_warnings(config) {
            tracing::warn!(%warning, "config warning");
        }
    }

    pub fn config_warnings(&self, config: &GaleConfig) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(profile) = config.profiles.get(&config.active_profile) {
            warnings.extend(Self::window_warnings(profile, config.tick_interval_ms));
        }

        let known = self.known_sensors.read().unwrap();
        if known.is_empty() {
            return warnings;
        }
        let Some(profile) = config.profiles.get(&config.active_profile) else {
            return warnings;
        };
        warnings.extend(Self::unwired_curve_warnings(profile));
        warnings.extend(
            profile
                .hardware_sensors_used()
                .into_iter()
                .filter(|sensor| !sensor.is_empty() && !known.contains(sensor))
                .map(|sensor| format!("referenced sensor not found on hardware: {sensor}")),
        );
        warnings
    }

    fn unwired_curve_warnings(profile: &ProfileConfig) -> Vec<String> {
        use gale_core::config::CurveConfig;
        profile
            .curves
            .iter()
            .filter_map(|(id, curve)| {
                let unwired = match curve {
                    CurveConfig::Point { sensor, .. }
                    | CurveConfig::Linear { sensor, .. }
                    | CurveConfig::Trigger { sensor, .. }
                    | CurveConfig::Target { sensor, .. } => sensor.is_empty(),
                    CurveConfig::Sync { source } | CurveConfig::Offset { source, .. } => {
                        source.is_empty()
                    }
                    CurveConfig::Mix { sources, .. } => sources.is_empty(),
                    CurveConfig::Flat { .. } => false,
                };
                unwired
                    .then(|| format!("curve '{id}' has no input wired and will fail safe to 100%"))
            })
            .collect()
    }

    fn window_warnings(profile: &ProfileConfig, tick_interval_ms: u64) -> Vec<String> {
        let tick_interval_s = tick_interval_ms as f64 / 1000.0;
        profile
            .sensors
            .iter()
            .filter_map(|(name, cfg)| {
                let window_s = match cfg {
                    VirtualSensorConfig::Mean {
                        window_s: Some(window_s),
                        ..
                    } => Some(*window_s),
                    VirtualSensorConfig::Delta { window_s, .. } => Some(*window_s),
                    _ => None,
                }?;
                if window_s < tick_interval_s {
                    Some(format!(
                        "virtual sensor '{name}' has window_s shorter than the tick interval and will not smooth or measure correctly"
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn tick(&self, dt_secs: f64) {
        let mut sensors = self.backend.read_all().await;
        let (mut duties, curves, manual, active_profile, assigned) = {
            let mut state = self.state.lock().unwrap();
            state.webhooks.seed(&mut sensors, Instant::now());
            let mut duties = state.engine.tick(&mut sensors, dt_secs);
            for (id, duty) in &state.manual {
                duties.insert(id.clone(), *duty);
            }
            let assigned: HashSet<Id> = state.engine.assignments().keys().cloned().collect();
            (
                duties,
                state.engine.curve_outputs().clone(),
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
                    self.write_journal().await;
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
                self.write_journal().await;
            }
        }

        let overrides: HashSet<Id> = manual.keys().cloned().collect();
        self.snapshot_tx.send_replace(Snapshot {
            sensors,
            duties,
            curves,
            manual,
            overrides,
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
            let HostState {
                config, webhooks, ..
            } = &mut *state;
            webhooks.reconcile(config);
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
                self.write_journal().await;
            }
        }
        Ok(())
    }

    pub async fn set_manual(&self, id: &str, duty: f64) -> Result<(), HwError> {
        self.backend.set_duty(id, duty).await?;
        self.claimed.lock().unwrap().insert(id.to_string());
        self.write_journal().await;
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
            self.write_journal().await;
        }
        Ok(())
    }

    pub fn record_webhook(&self, token: &str, value: f64) -> bool {
        self.state
            .lock()
            .unwrap()
            .webhooks
            .record(token, value, Instant::now())
    }

    pub fn webhook_token(&self, name: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .webhooks
            .token_for(name)
            .map(str::to_string)
    }

    pub fn claimed_ids(&self) -> Vec<Id> {
        self.claimed.lock().unwrap().iter().cloned().collect()
    }

    async fn write_journal(&self) {
        let mut entries = Vec::new();
        for id in self.claimed_ids() {
            let backend_kind = claims_journal::backend_kind_of(&id);
            let hint = if claims_journal::kind_has_restore_hint(&backend_kind) {
                self.backend.restore_hint(&id).await
            } else {
                None
            };
            entries.push(JournalEntry {
                id,
                backend_kind,
                hint,
            });
        }
        let path = claims_journal::default_path();
        if let Err(error) = claims_journal::write(&path, &entries) {
            tracing::warn!(%error, path = %path.display(), "failed to write claim journal");
        }
    }

    pub fn blocking_release_claimed(&self) {
        for id in self.claimed_ids() {
            self.backend.blocking_release(&id);
        }
    }

    pub async fn release_all(&self) {
        let ids = self.claimed_ids();
        for id in ids {
            match self.backend.release(&id).await {
                Ok(()) => {
                    self.claimed.lock().unwrap().remove(&id);
                }
                Err(error) => {
                    tracing::warn!(%id, %error, "release failed");
                }
            }
        }
        self.write_journal().await;
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
        setup_with_config(CONFIG, sensors)
    }

    fn setup_with_config(
        config: &str,
        sensors: &[(&str, Option<f64>)],
    ) -> (Arc<EngineHost>, Arc<Mutex<Recorded>>) {
        let state = Arc::new(Mutex::new(Recorded {
            sensors: sensors.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            ..Recorded::default()
        }));
        let handle = BackendHandle::spawn(Box::new(RecordingBackend {
            state: state.clone(),
        }));
        let pool = BackendPool::new(vec![handle]);
        let host = EngineHost::new(GaleConfig::from_toml(config).unwrap(), pool).unwrap();
        (host, state)
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn tick_applies_curve_duties_and_publishes_snapshot() {
        let _guard = crate::test_support::lock_env();
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        assert_eq!(state.lock().unwrap().duties["pwm1"], 60.0);
        assert_eq!(state.lock().unwrap().duties["pwm2"], 60.0);
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["t1"], Some(50.0));
        assert_eq!(snapshot.duties["pwm1"], 60.0);
        assert_eq!(snapshot.curves["cpu"], 60.0);
        assert_eq!(snapshot.active_profile, "p");
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn manual_override_beats_curve_and_write_errors_are_skipped() {
        let _guard = crate::test_support::lock_env();
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
    #[allow(clippy::await_holding_lock)]
    async fn snapshot_overrides_lists_manually_overridden_controls_and_clears() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.set_manual("pwm1", 33.0).await.unwrap();
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert!(snapshot.overrides.contains("pwm1"));
        assert!(!snapshot.overrides.contains("pwm2"));

        host.clear_manual("pwm1").await.unwrap();
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert!(!snapshot.overrides.contains("pwm1"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn snapshot_overrides_is_empty_when_no_control_is_overridden() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert!(snapshot.overrides.is_empty());
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn clear_manual_release_is_not_reclaimed_by_next_tick() {
        let _guard = crate::test_support::lock_env();
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
    #[allow(clippy::await_holding_lock)]
    async fn stale_claim_from_replaced_config_is_released_exactly_once() {
        let _guard = crate::test_support::lock_env();
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
    #[allow(clippy::await_holding_lock)]
    async fn invalid_replacement_config_keeps_old_engine_running() {
        let _guard = crate::test_support::lock_env();
        let (host, state) = setup(&[("t1", Some(50.0))]);
        let mut bad = GaleConfig::from_toml(CONFIG).unwrap();
        bad.active_profile = "ghost".into();
        assert!(host.replace_config(bad).await.is_err());
        host.tick(1.0).await;
        assert_eq!(state.lock().unwrap().duties["pwm1"], 60.0);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn replacement_releases_controls_dropped_from_new_config() {
        let _guard = crate::test_support::lock_env();
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
    #[allow(clippy::await_holding_lock)]
    async fn clear_manual_on_unassigned_control_releases_it() {
        let _guard = crate::test_support::lock_env();
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
    #[allow(clippy::await_holding_lock)]
    async fn claimed_ids_returns_claimed_set_after_tick() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        let mut ids = host.claimed_ids();
        ids.sort();
        assert_eq!(ids, vec!["pwm1".to_string(), "pwm2".to_string()]);
    }

    #[test]
    fn warnings_puts_platform_warnings_before_config_warnings() {
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.set_known_sensors(["other".to_string()].into());
        host.set_platform_warnings(vec!["a https://pawnio.eu".to_string()]);
        let config = GaleConfig::from_toml(CONFIG).unwrap();
        let warnings = host.warnings(&config);
        assert_eq!(warnings[0], "a https://pawnio.eu");
        assert_eq!(warnings.len(), 2);
        assert!(warnings[1].contains("t1"));
        assert!(!host
            .config_warnings(&config)
            .iter()
            .any(|warning| warning.contains("pawnio")));
    }

    #[test]
    fn config_warnings_empty_when_known_sensors_never_set() {
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        let config = GaleConfig::from_toml(CONFIG).unwrap();
        assert!(host.config_warnings(&config).is_empty());
    }

    #[test]
    fn config_warnings_flags_referenced_but_unknown_sensor() {
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.set_known_sensors(["other".to_string()].into());
        let config = GaleConfig::from_toml(CONFIG).unwrap();
        let warnings = host.config_warnings(&config);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("t1"));
    }

    #[test]
    fn config_warnings_ignores_unknown_sensor_on_unassigned_curve() {
        const CONFIG_WITH_DRAFT: &str = r#"
active_profile = "p"

[profiles.p.curves.cpu]
type = "point"
sensor = "t1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.curves.draft]
type = "point"
sensor = "hwmon/nct6798/temp1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.assignments]
"pwm1" = "cpu"
"pwm2" = "cpu"
"#;
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.set_known_sensors(["t1".to_string()].into());
        let config = GaleConfig::from_toml(CONFIG_WITH_DRAFT).unwrap();
        assert!(host.config_warnings(&config).is_empty());
    }

    #[test]
    fn config_warnings_empty_when_known_sensor_matches() {
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        host.set_known_sensors(["t1".to_string()].into());
        let config = GaleConfig::from_toml(CONFIG).unwrap();
        assert!(host.config_warnings(&config).is_empty());
    }

    const VIRTUAL_CONFIG: &str = r#"
active_profile = "p"

[profiles.p.sensors.cpu_hot]
type = "max"
inputs = ["t1", "t2"]

[profiles.p.curves.cpu]
type = "point"
sensor = "virtual/cpu_hot"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.assignments]
"pwm1" = "cpu"
"#;

    #[test]
    fn config_warnings_flags_windowed_sensors_shorter_than_the_tick_interval() {
        const CONFIG_SHORT_WINDOWS: &str = r#"
tick_interval_ms = 1000
active_profile = "p"

[profiles.p.curves.c]
type = "flat"
duty = 50.0

[profiles.p.assignments]
"pwm1" = "c"

[profiles.p.sensors.coolant_smooth]
type = "mean"
inputs = ["t1"]
window_s = 0.5

[profiles.p.sensors.coolant_rise]
type = "delta"
input = "t1"
window_s = 0.9
"#;
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        let config = GaleConfig::from_toml(CONFIG_SHORT_WINDOWS).unwrap();
        let warnings = host.config_warnings(&config);
        assert_eq!(warnings.len(), 2);
        assert!(warnings
            .iter()
            .any(|w| w.contains("coolant_smooth") && w.contains("tick interval")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("coolant_rise") && w.contains("tick interval")));
    }

    #[test]
    fn config_warnings_does_not_flag_windows_at_or_above_the_tick_interval() {
        const CONFIG_LONG_WINDOWS: &str = r#"
tick_interval_ms = 1000
active_profile = "p"

[profiles.p.curves.c]
type = "flat"
duty = 50.0

[profiles.p.assignments]
"pwm1" = "c"

[profiles.p.sensors.coolant_smooth]
type = "mean"
inputs = ["t1"]
window_s = 1.0

[profiles.p.sensors.coolant_rise]
type = "delta"
input = "t1"
window_s = 2.0
"#;
        let (host, _state) = setup(&[("t1", Some(50.0))]);
        let config = GaleConfig::from_toml(CONFIG_LONG_WINDOWS).unwrap();
        assert!(host.config_warnings(&config).is_empty());
    }

    #[test]
    fn config_warnings_never_flags_non_windowed_virtual_sensors() {
        const CONFIG_NON_WINDOWED: &str = r#"
tick_interval_ms = 100
active_profile = "p"

[profiles.p.curves.c]
type = "flat"
duty = 50.0

[profiles.p.assignments]
"pwm1" = "c"

[profiles.p.sensors.cpu_hot]
type = "max"
inputs = ["t1", "t2"]
"#;
        let (host, _state) = setup(&[("t1", Some(50.0)), ("t2", Some(60.0))]);
        let config = GaleConfig::from_toml(CONFIG_NON_WINDOWED).unwrap();
        assert!(host.config_warnings(&config).is_empty());
    }

    #[test]
    fn config_warnings_resolve_virtual_sensors_to_hardware_inputs() {
        let (host, _state) = setup(&[("t1", Some(50.0)), ("t2", Some(60.0))]);
        host.set_known_sensors(["t1".to_string()].into());
        let config = GaleConfig::from_toml(VIRTUAL_CONFIG).unwrap();
        let warnings = host.config_warnings(&config);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("t2"));
        assert!(!warnings.iter().any(|w| w.contains("virtual")));
    }

    #[test]
    fn config_warnings_never_mention_virtual_ids_even_when_inventory_is_empty_of_them() {
        let (host, _state) = setup(&[("t1", Some(50.0)), ("t2", Some(60.0))]);
        host.set_known_sensors(["t1".to_string(), "t2".to_string()].into());
        let config = GaleConfig::from_toml(VIRTUAL_CONFIG).unwrap();
        assert!(host.config_warnings(&config).is_empty());
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn snapshot_sensors_include_virtual_values() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) =
            setup_with_config(VIRTUAL_CONFIG, &[("t1", Some(50.0)), ("t2", Some(58.0))]);
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["virtual/cpu_hot"], Some(58.0));
        assert_eq!(snapshot.duties["pwm1"], 76.0);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn claiming_a_superio_control_journals_its_restore_hint() {
        let _lock = crate::test_support::lock_env();
        let runtime_dir = tempfile::tempdir().unwrap();
        let mut env = crate::test_support::EnvVarGuard::new();
        env.set("GALE_RUNTIME_DIR", runtime_dir.path());

        let (host, state) = setup(&[("t1", Some(50.0))]);
        state.lock().unwrap().hint =
            Some(("superio".to_string(), "nct6798d:pwm1:42:7f".to_string()));

        host.set_manual("superio/nct6798d/pwm1", 60.0)
            .await
            .unwrap();

        let journal_path = runtime_dir.path().join("claims.json");
        let entries = claims_journal::load(&journal_path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "superio/nct6798d/pwm1");
        assert_eq!(entries[0].backend_kind, "superio");
        assert_eq!(
            entries[0].hint,
            Some(("superio".to_string(), "nct6798d:pwm1:42:7f".to_string()))
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn claiming_a_hwmon_control_journals_its_restore_hint_and_survives_unclean_death() {
        use crate::backend_handle::BackendHandle;
        use gale_hw_hwmon::HwmonBackend;
        use std::fs;

        let _guard = crate::test_support::lock_env();
        let runtime_dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("GALE_RUNTIME_DIR", runtime_dir.path());
        }

        let hwmon_root = tempfile::tempdir().unwrap();
        let chip0 = hwmon_root.path().join("hwmon0");
        fs::create_dir(&chip0).unwrap();
        fs::write(chip0.join("name"), "nct6798\n").unwrap();
        fs::write(chip0.join("pwm1"), "128\n").unwrap();
        fs::write(chip0.join("pwm1_enable"), "5\n").unwrap();

        let handle = BackendHandle::spawn(Box::new(HwmonBackend::with_root(
            hwmon_root.path().to_path_buf(),
        )));
        let pool = BackendPool::new(vec![handle]);
        pool.enumerate().await;

        let host = EngineHost::new(GaleConfig::from_toml(CONFIG).unwrap(), pool).unwrap();
        host.set_manual("hwmon/nct6798/pwm1", 60.0).await.unwrap();

        let journal_path = runtime_dir.path().join("claims.json");
        let entries = claims_journal::load(&journal_path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "hwmon/nct6798/pwm1");
        assert_eq!(entries[0].backend_kind, "hwmon");
        let (hint_path, hint_value) = entries[0].hint.clone().unwrap();
        assert_eq!(hint_path, chip0.join("pwm1_enable").display().to_string());
        assert_eq!(hint_value, "5");

        drop(host);

        let restored = claims_journal::run_restore(&journal_path);
        assert_eq!(restored, 1);
        assert_eq!(fs::read_to_string(chip0.join("pwm1_enable")).unwrap(), "5");
        assert!(!journal_path.exists());

        unsafe {
            std::env::remove_var("GALE_RUNTIME_DIR");
        }
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn release_all_releases_every_claimed_control_once() {
        let _guard = crate::test_support::lock_env();
        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        host.release_all().await;
        let mut released = state.lock().unwrap().released.clone();
        released.sort();
        assert_eq!(released, vec!["pwm1".to_string(), "pwm2".to_string()]);
        host.release_all().await;
        assert_eq!(state.lock().unwrap().released.len(), 2);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn release_failure_during_release_all_keeps_journal_entry_for_that_id() {
        let _guard = crate::test_support::lock_env();
        let runtime_dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("GALE_RUNTIME_DIR", runtime_dir.path());
        }

        let (host, state) = setup(&[("t1", Some(50.0))]);
        host.tick(1.0).await;
        state
            .lock()
            .unwrap()
            .fail_release_ids
            .push("pwm2".to_string());

        host.release_all().await;

        let journal_path = runtime_dir.path().join("claims.json");
        let entries = claims_journal::load(&journal_path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "pwm2");

        unsafe {
            std::env::remove_var("GALE_RUNTIME_DIR");
        }
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER_TOKEN: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    const WEBHOOK_HOST_CONFIG: &str = r#"
active_profile = "p"

[profiles.p.sensors.hook]
type = "webhook"
token = "{TOKEN}"
timeout_s = 0.05

[profiles.p.sensors.forever]
type = "webhook"
token = "{OTHER_TOKEN}"

[profiles.p.curves.remote]
type = "point"
sensor = "virtual/hook"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.curves.steady]
type = "point"
sensor = "virtual/forever"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.assignments]
"pwm1" = "remote"
"pwm2" = "steady"
"#;

    fn with_tokens(toml: &str) -> String {
        toml.replace("{TOKEN}", TOKEN)
            .replace("{OTHER_TOKEN}", OTHER_TOKEN)
    }

    fn setup_webhooks() -> (Arc<EngineHost>, Arc<Mutex<Recorded>>) {
        setup_with_config(&with_tokens(WEBHOOK_HOST_CONFIG), &[("t1", Some(50.0))])
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn webhook_value_flows_into_snapshot_and_curve_after_record() {
        let _guard = crate::test_support::lock_env();
        let (host, state) = setup_webhooks();
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["virtual/hook"], None);
        assert_eq!(snapshot.duties["pwm1"], 100.0);

        assert!(host.record_webhook(TOKEN, 51.5));
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["virtual/hook"], Some(51.5));
        assert_eq!(snapshot.duties["pwm1"], 63.0);
        assert_eq!(state.lock().unwrap().duties["pwm1"], 63.0);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn webhook_value_goes_stale_after_timeout_and_curve_fails_safe() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup_webhooks();
        host.record_webhook(TOKEN, 51.5);
        host.tick(1.0).await;
        assert_eq!(
            host.subscribe().borrow().sensors["virtual/hook"],
            Some(51.5)
        );

        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["virtual/hook"], None);
        assert_eq!(snapshot.duties["pwm1"], 100.0);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn webhook_without_timeout_never_goes_stale() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup_webhooks();
        host.record_webhook(OTHER_TOKEN, 40.0);
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["virtual/forever"], Some(40.0));
        assert_eq!(snapshot.duties["pwm2"], 40.0);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn record_webhook_with_unknown_token_returns_false() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup_webhooks();
        assert!(!host.record_webhook(&"f".repeat(64), 1.0));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn replace_config_keeps_webhook_state_for_unchanged_token_and_resets_renamed() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup_webhooks();
        host.record_webhook(OTHER_TOKEN, 40.0);
        host.record_webhook(TOKEN, 51.5);

        let renamed = WEBHOOK_HOST_CONFIG
            .replace("sensors.hook]", "sensors.hook2]")
            .replace("sensor = \"virtual/hook\"", "sensor = \"virtual/hook2\"")
            .replace(
                "token = \"{OTHER_TOKEN}\"",
                "token = \"{OTHER_TOKEN}\"\ntimeout_s = 600.0",
            );
        host.replace_config(GaleConfig::from_toml(&with_tokens(&renamed)).unwrap())
            .await
            .unwrap();

        host.tick(1.0).await;
        let snapshot = host.subscribe().borrow().clone();
        assert_eq!(snapshot.sensors["virtual/forever"], Some(40.0));
        assert_eq!(snapshot.sensors["virtual/hook2"], None);
        assert!(!snapshot.sensors.contains_key("virtual/hook"));
        assert_eq!(snapshot.duties["pwm1"], 100.0);
        assert_eq!(snapshot.duties["pwm2"], 40.0);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn webhook_token_returns_token_for_webhook_sensors_only() {
        let _guard = crate::test_support::lock_env();
        let (host, _state) = setup_webhooks();
        assert_eq!(host.webhook_token("hook"), Some(TOKEN.to_string()));
        assert_eq!(host.webhook_token("remote"), None);
        assert_eq!(host.webhook_token("ghost"), None);

        let (host, _state) = setup_with_config(VIRTUAL_CONFIG, &[("t1", Some(50.0))]);
        assert_eq!(host.webhook_token("cpu_hot"), None);
    }
}
