use gale_core::config::ConfigError;
use gale_hw_corsair::CorsairBackend;
use gale_hw_corsair::ReleaseMode;
#[cfg(target_os = "linux")]
use gale_hw_hwmon::HwmonBackend;
use gale_hw_nvidia::NvidiaBackend;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use crate::api::{self, ApiContext};
use crate::backend_handle::BackendHandle;
use crate::backend_pool::BackendPool;
use crate::config_store::ConfigStore;
use crate::engine_host::EngineHost;
use crate::runtime;
use crate::shutdown::ShutdownSignal;

pub struct DaemonOptions {
    pub shutdown: ShutdownSignal,
    pub on_ready: Option<Box<dyn FnOnce(SocketAddr) + Send>>,
    pub on_host_ready: Option<Box<dyn FnOnce(Arc<EngineHost>) + Send>>,
}

#[derive(Debug)]
pub enum DaemonError {
    ConfigLoad(PathBuf, ConfigError),
    ConfigInvalid(ConfigError),
    Bind(String, std::io::Error),
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonError::ConfigLoad(path, error) => {
                write!(f, "failed to load config {}: {error}", path.display())
            }
            DaemonError::ConfigInvalid(error) => write!(f, "invalid config: {error}"),
            DaemonError::Bind(bind, error) => write!(f, "cannot bind {bind}: {error}"),
        }
    }
}

impl std::error::Error for DaemonError {}

pub async fn run(options: DaemonOptions) -> Result<(), DaemonError> {
    let mut shutdown = options.shutdown;

    if let Err(error) = crate::paths::ensure_dirs() {
        tracing::warn!(%error, "failed to create default gale directories");
    }

    let restore_path = crate::claims_journal::default_path();
    let restored = crate::claims_journal::run_restore(&restore_path);
    if restored > 0 {
        tracing::info!(restored, path = %restore_path.display(), "restored claims left by a previous run");
    }

    let store = Arc::new(ConfigStore::new(ConfigStore::default_path()));
    let config = store
        .load()
        .map_err(|error| DaemonError::ConfigLoad(store.path().to_path_buf(), error))?;
    let mut handles: Vec<BackendHandle> = Vec::new();
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut platform_warnings: Vec<String> = Vec::new();
    #[cfg(target_os = "linux")]
    handles.push(BackendHandle::spawn(Box::new(HwmonBackend::new())));
    let corsair_release_mode: ReleaseMode =
        crate::claims_journal::release_mode_from_config(&config.hardware.corsair.on_release);
    for backend in CorsairBackend::open_all(corsair_release_mode) {
        handles.push(BackendHandle::spawn(Box::new(backend)));
    }
    handles.push(BackendHandle::spawn(Box::new(NvidiaBackend::new())));
    #[cfg(windows)]
    match tokio::task::spawn_blocking(gale_hw_superio::probe).await {
        Ok(Ok(backend)) => handles.push(BackendHandle::spawn(Box::new(backend))),
        Ok(Err(status)) => {
            tracing::warn!(?status, "super i/o backend unavailable");
            if let Some(text) = status.warning_text() {
                platform_warnings.push(text);
            }
        }
        Err(join_error) => {
            tracing::warn!(%join_error, "super i/o probe task panicked");
            platform_warnings.push(
                "Motherboard fan control is unavailable: the PawnIO probe panicked. See https://pawnio.eu."
                    .to_string(),
            );
        }
    }
    #[cfg(windows)]
    {
        let dimm_enabled = config.hardware.dimm.enabled;
        match tokio::task::spawn_blocking(gale_hw_amdcpu::probe).await {
            Ok(Ok(backend)) => handles.push(BackendHandle::spawn(Box::new(backend))),
            Ok(Err(status)) => {
                tracing::info!(?status, "amd cpu backend unavailable");
                platform_warnings.extend(status.warning_text());
            }
            Err(join_error) => tracing::warn!(%join_error, "amd cpu probe task panicked"),
        }
        match tokio::task::spawn_blocking(move || gale_hw_dimm::probe(dimm_enabled)).await {
            Ok(Ok(backend)) => handles.push(BackendHandle::spawn(Box::new(backend))),
            Ok(Err(status)) => {
                tracing::info!(?status, "dimm backend unavailable");
                platform_warnings.extend(status.warning_text());
            }
            Err(join_error) => tracing::warn!(%join_error, "dimm probe task panicked"),
        }
        match tokio::task::spawn_blocking(gale_hw_asus_ec::probe).await {
            Ok(Ok(backend)) => handles.push(BackendHandle::spawn(Box::new(backend))),
            Ok(Err(status)) => {
                tracing::info!(?status, "asus ec backend unavailable");
                platform_warnings.extend(status.warning_text());
            }
            Err(join_error) => tracing::warn!(%join_error, "asus ec probe task panicked"),
        }
    }
    let pool = BackendPool::new(handles);
    let inventory = pool.enumerate().await;
    tracing::info!(
        sensors = inventory.sensors.len(),
        controls = inventory.controls.len(),
        "hardware enumerated"
    );
    let host = EngineHost::new(config.clone(), pool).map_err(DaemonError::ConfigInvalid)?;
    host.set_known_sensors(
        inventory
            .sensors
            .iter()
            .map(|sensor| sensor.id.clone())
            .collect(),
    );
    host.set_platform_warnings(platform_warnings);
    host.log_config_warnings(&config);
    if let Some(on_host_ready) = options.on_host_ready {
        on_host_ready(host.clone());
    }
    install_panic_release_hook(host.clone());
    let heartbeat = Arc::new(Mutex::new(Instant::now()));
    let tick_handle = tokio::spawn(runtime::tick_loop(
        host.clone(),
        config.tick_interval_ms,
        heartbeat.clone(),
    ));
    let watchdog_handle = tokio::spawn(runtime::watchdog(
        host.clone(),
        config.tick_interval_ms,
        heartbeat,
    ));
    let _watcher = match runtime::spawn_config_watcher(store.clone(), host.clone()) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            tracing::warn!(%error, "config hot-reload unavailable");
            None
        }
    };
    let ctx = ApiContext {
        host: host.clone(),
        store,
        inventory: Arc::new(RwLock::new(inventory)),
        api_key: config.api.api_key.clone(),
    };
    let listener = match tokio::net::TcpListener::bind(&config.api.bind).await {
        Ok(listener) => listener,
        Err(error) => {
            tick_handle.abort();
            watchdog_handle.abort();
            host.release_all().await;
            return Err(DaemonError::Bind(config.api.bind.clone(), error));
        }
    };
    let bound_addr = listener
        .local_addr()
        .expect("bound tcp listener has a local address");
    tracing::info!(bind = %bound_addr, "gale daemon listening");
    if let Some(on_ready) = options.on_ready {
        on_ready(bound_addr);
    }
    let server = axum::serve(listener, api::router(ctx));
    tokio::select! {
        result = server => {
            if let Err(error) = result {
                tracing::error!(%error, "server error");
            }
        }
        _ = shutdown.wait() => {}
    }
    tick_handle.abort();
    watchdog_handle.abort();
    tracing::info!("shutting down, releasing all controls");
    host.release_all().await;
    Ok(())
}

fn install_panic_release_hook(host: Arc<EngineHost>) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("panic detected, attempting best-effort release of claimed controls: {info}");
        let host = host.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let _ = std::thread::spawn(move || {
            host.blocking_release_claimed();
            let _ = done_tx.send(());
        });
        let _ = done_rx.recv_timeout(std::time::Duration::from_secs(2));
        previous_hook(info);
    }));
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use super::*;
    use crate::shutdown;
    use std::fs;
    use std::io::Write;

    fn write_file(path: &std::path::Path, contents: &str) {
        let mut file = fs::File::create(path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn run_binds_ephemeral_port_and_stops_on_pre_fired_signal() {
        let _lock = crate::test_support::lock_env();

        let workdir = tempfile::tempdir().unwrap();

        let chip0 = workdir.path().join("hwmon/hwmon0");
        fs::create_dir_all(&chip0).unwrap();
        write_file(&chip0.join("name"), "nct6798\n");
        write_file(&chip0.join("temp1_input"), "45000\n");
        write_file(&chip0.join("temp1_label"), "CPUTIN\n");

        let runtime_dir = workdir.path().join("run");
        fs::create_dir_all(&runtime_dir).unwrap();

        let config_path = workdir.path().join("config.toml");
        write_file(
            &config_path,
            r#"
tick_interval_ms = 1000
active_profile = "default"

[api]
bind = "127.0.0.1:0"

[profiles.default.curves]
[profiles.default.assignments]
"#,
        );

        let mut env = crate::test_support::EnvVarGuard::new();
        env.set("GALE_HWMON_ROOT", workdir.path().join("hwmon"));
        env.set("GALE_CONFIG", &config_path);
        env.set("GALE_RUNTIME_DIR", &runtime_dir);

        let (trigger, signal) = shutdown::channel();
        trigger.fire();

        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (host_tx, host_rx) = std::sync::mpsc::channel();
        let options = DaemonOptions {
            shutdown: signal,
            on_ready: Some(Box::new(move |addr| {
                let _ = ready_tx.send(addr);
            })),
            on_host_ready: Some(Box::new(move |host| {
                let _ = host_tx.send(host);
            })),
        };

        run(options).await.unwrap();

        let addr = ready_rx.try_recv().expect("on_ready was not called");
        assert_ne!(addr.port(), 0);

        #[cfg(windows)]
        {
            let host = host_rx.try_recv().expect("on_host_ready was not called");
            let config = host.config();
            let warnings = host.warnings(&config);
            assert!(warnings
                .iter()
                .any(|warning| warning.contains("https://pawnio.eu")));
        }
        #[cfg(not(windows))]
        drop(host_rx);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn run_stops_ticking_after_shutdown_so_released_controls_stay_released() {
        let _lock = crate::test_support::lock_env();

        let workdir = tempfile::tempdir().unwrap();

        let chip0 = workdir.path().join("hwmon/hwmon0");
        fs::create_dir_all(&chip0).unwrap();
        write_file(&chip0.join("name"), "nct6798\n");
        write_file(&chip0.join("temp1_input"), "45000\n");
        write_file(&chip0.join("temp1_label"), "CPUTIN\n");
        write_file(&chip0.join("fan1_input"), "1200\n");
        write_file(&chip0.join("pwm1"), "128\n");
        write_file(&chip0.join("pwm1_mode"), "1\n");
        write_file(&chip0.join("pwm1_enable"), "5\n");

        let runtime_dir = workdir.path().join("run");
        fs::create_dir_all(&runtime_dir).unwrap();

        let config_path = workdir.path().join("config.toml");
        write_file(
            &config_path,
            r#"
tick_interval_ms = 10
active_profile = "default"

[api]
bind = "127.0.0.1:0"

[profiles.default.curves.cpu]
type = "point"
sensor = "hwmon/nct6798/temp1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.default.assignments]
"hwmon/nct6798/pwm1" = "cpu"
"#,
        );

        let mut env = crate::test_support::EnvVarGuard::new();
        env.set("GALE_HWMON_ROOT", workdir.path().join("hwmon"));
        env.set("GALE_CONFIG", &config_path);
        env.set("GALE_RUNTIME_DIR", &runtime_dir);

        let (trigger, signal) = shutdown::channel();
        let on_ready: Box<dyn FnOnce(SocketAddr) + Send> = Box::new(move |_addr| {
            trigger.fire();
        });
        let options = DaemonOptions {
            shutdown: signal,
            on_ready: Some(on_ready),
            on_host_ready: None,
        };

        run(options).await.unwrap();

        let enable_path = chip0.join("pwm1_enable");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let enable_after_wait = fs::read_to_string(&enable_path).unwrap();
        assert_eq!(
            enable_after_wait.trim(),
            "5",
            "tick loop kept running after shutdown and reclaimed a released control"
        );
    }
}
