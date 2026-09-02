use gale_hw_corsair::CorsairBackend;
#[cfg(target_os = "linux")]
use gale_hw_hwmon::HwmonBackend;
use gale_hw_nvidia::NvidiaBackend;
use std::net::SocketAddr;
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
}

pub async fn run(options: DaemonOptions) {
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
    let config = match store.load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load config {}: {error}", store.path().display());
            std::process::exit(1);
        }
    };
    let mut handles: Vec<BackendHandle> = Vec::new();
    #[cfg(target_os = "linux")]
    handles.push(BackendHandle::spawn(Box::new(HwmonBackend::new())));
    for backend in CorsairBackend::open_all() {
        handles.push(BackendHandle::spawn(Box::new(backend)));
    }
    handles.push(BackendHandle::spawn(Box::new(NvidiaBackend::new())));
    let pool = BackendPool::new(handles);
    let inventory = pool.enumerate().await;
    tracing::info!(
        sensors = inventory.sensors.len(),
        controls = inventory.controls.len(),
        "hardware enumerated"
    );
    let host = match EngineHost::new(config.clone(), pool) {
        Ok(host) => host,
        Err(error) => {
            eprintln!("invalid config: {error}");
            std::process::exit(1);
        }
    };
    host.set_known_sensors(
        inventory
            .sensors
            .iter()
            .map(|sensor| sensor.id.clone())
            .collect(),
    );
    host.log_config_warnings(&config);
    install_panic_release_hook(host.clone());
    let heartbeat = Arc::new(Mutex::new(Instant::now()));
    tokio::spawn(runtime::tick_loop(
        host.clone(),
        config.tick_interval_ms,
        heartbeat.clone(),
    ));
    tokio::spawn(runtime::watchdog(
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
            eprintln!("cannot bind {}: {error}", config.api.bind);
            std::process::exit(1);
        }
    };
    let bound_addr = listener.local_addr().ok();
    tracing::info!(bind = %config.api.bind, "gale daemon listening");
    if let Some(on_ready) = options.on_ready {
        if let Some(addr) = bound_addr {
            on_ready(addr);
        }
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
    tracing::info!("shutting down, releasing all controls");
    host.release_all().await;
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

#[cfg(all(test, target_os = "linux"))]
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
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();

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

        unsafe {
            std::env::set_var("GALE_HWMON_ROOT", workdir.path().join("hwmon"));
            std::env::set_var("GALE_CONFIG", &config_path);
            std::env::set_var("GALE_RUNTIME_DIR", &runtime_dir);
        }

        let (trigger, signal) = shutdown::channel();
        trigger.fire();

        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let options = DaemonOptions {
            shutdown: signal,
            on_ready: Some(Box::new(move |addr| {
                let _ = ready_tx.send(addr);
            })),
        };

        run(options).await;

        let addr = ready_rx.try_recv().expect("on_ready was not called");
        assert_ne!(addr.port(), 0);

        unsafe {
            std::env::remove_var("GALE_HWMON_ROOT");
            std::env::remove_var("GALE_CONFIG");
            std::env::remove_var("GALE_RUNTIME_DIR");
        }
    }
}
