use gale_hw_corsair::CorsairBackend;
use gale_hw_hwmon::HwmonBackend;
use gale_hw_nvidia::NvidiaBackend;
use galed::api::{self, ApiContext};
use galed::backend_handle::BackendHandle;
use galed::backend_pool::BackendPool;
use galed::config_store::ConfigStore;
use galed::engine_host::EngineHost;
use galed::runtime;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let store = Arc::new(ConfigStore::new(ConfigStore::default_path()));
    let config = match store.load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load config {}: {error}", store.path().display());
            std::process::exit(1);
        }
    };
    let mut handles = vec![BackendHandle::spawn(Box::new(HwmonBackend::new()))];
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
    for warning in host.config_warnings(&config) {
        tracing::warn!(%warning, "config warning");
    }
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
    tracing::info!(bind = %config.api.bind, "gale daemon listening");
    let server = axum::serve(listener, api::router(ctx));
    tokio::select! {
        result = server => {
            if let Err(error) = result {
                tracing::error!(%error, "server error");
            }
        }
        _ = shutdown_signal() => {}
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

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
}
