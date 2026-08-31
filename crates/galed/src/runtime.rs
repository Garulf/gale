use crate::config_store::ConfigStore;
use crate::engine_host::EngineHost;
use notify::{RecursiveMode, Watcher};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub async fn tick_loop(host: Arc<EngineHost>, interval_ms: u64, heartbeat: Arc<Mutex<Instant>>) {
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms.max(50)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last = Instant::now();
    loop {
        interval.tick().await;
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64();
        last = now;
        host.tick(dt).await;
        *heartbeat.lock().unwrap() = Instant::now();
    }
}

pub async fn watchdog(host: Arc<EngineHost>, interval_ms: u64, heartbeat: Arc<Mutex<Instant>>) {
    let interval_duration = Duration::from_millis(interval_ms.max(50));
    let threshold = interval_duration * 5;
    let mut interval = tokio::time::interval(interval_duration);
    loop {
        interval.tick().await;
        let stalled = heartbeat.lock().unwrap().elapsed() > threshold;
        if stalled {
            tracing::error!("tick loop stalled, releasing all controls");
            host.release_all().await;
        }
    }
}

pub fn spawn_config_watcher(
    store: Arc<ConfigStore>,
    host: Arc<EngineHost>,
) -> notify::Result<notify::RecommendedWatcher> {
    let config_path = store.path().to_path_buf();
    let watch_dir = config_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let runtime = tokio::runtime::Handle::current();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        if !event.paths.iter().any(|p| p.ends_with(config_path.file_name().unwrap_or_default())) {
            return;
        }
        if store.recently_saved(Duration::from_secs(2)) {
            return;
        }
        let store = store.clone();
        let host = host.clone();
        runtime.spawn(async move {
            match store.load() {
                Ok(config) => {
                    if let Err(error) = host.replace_config(config).await {
                        tracing::warn!(%error, "hot-reload rejected invalid config");
                    } else {
                        tracing::info!("config hot-reloaded");
                    }
                }
                Err(error) => tracing::warn!(%error, "hot-reload could not parse config"),
            }
        });
    })?;
    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend_handle::BackendHandle;
    use crate::engine_host::EngineHost;
    use crate::test_support::{Recorded, RecordingBackend};
    use gale_core::config::GaleConfig;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    const CONFIG: &str = r#"
active_profile = "p"

[profiles.p.curves.flat]
type = "flat"
duty = 42.0

[profiles.p.assignments]
"pwm1" = "flat"
"#;

    fn setup() -> (Arc<EngineHost>, Arc<Mutex<Recorded>>) {
        let state = Arc::new(Mutex::new(Recorded::default()));
        let handle = BackendHandle::spawn(Box::new(RecordingBackend { state: state.clone() }));
        (EngineHost::new(GaleConfig::from_toml(CONFIG).unwrap(), handle).unwrap(), state)
    }

    #[tokio::test]
    async fn watchdog_releases_all_when_heartbeat_stalls() {
        let (host, state) = setup();
        host.tick(1.0).await;
        let heartbeat = Arc::new(Mutex::new(Instant::now()));
        let stale = *heartbeat.lock().unwrap() - Duration::from_secs(60);
        *heartbeat.lock().unwrap() = stale;
        let watchdog_host = host.clone();
        let watchdog_beat = heartbeat.clone();
        let task = tokio::spawn(watchdog(watchdog_host, 100, watchdog_beat));
        tokio::time::sleep(Duration::from_millis(400)).await;
        task.abort();
        assert_eq!(state.lock().unwrap().released, vec!["pwm1".to_string()]);
    }

    #[tokio::test]
    async fn tick_loop_ticks_and_updates_heartbeat() {
        let (host, state) = setup();
        let heartbeat = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(60)));
        let task = tokio::spawn(tick_loop(host, 50, heartbeat.clone()));
        tokio::time::sleep(Duration::from_millis(300)).await;
        task.abort();
        assert!(heartbeat.lock().unwrap().elapsed() < Duration::from_secs(5));
        assert_eq!(state.lock().unwrap().duties["pwm1"], 42.0);
    }
}
