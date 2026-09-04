use crate::config_store::ConfigStore;
use crate::engine_host::EngineHost;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use gale_core::build::validate_profiles;
use gale_core::config::{virtual_id, ConfigError, GaleConfig};
use gale_hw::{Id, Inventory};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct ApiContext {
    pub host: Arc<EngineHost>,
    pub store: Arc<ConfigStore>,
    pub inventory: Arc<RwLock<Inventory>>,
    pub api_key: Option<String>,
}

#[derive(Deserialize)]
struct DutyBody {
    duty: f64,
}

pub fn router(ctx: ApiContext) -> Router {
    let api = Router::new()
        .route("/status", get(status))
        .route("/inventory", get(inventory))
        .route("/config", get(get_config).put(put_config))
        .route("/warnings", get(get_warnings))
        .route("/profiles/:name/activate", post(activate_profile))
        .route("/controls/*id", put(set_control).delete(clear_control))
        .route("/ws", get(ws_upgrade))
        .fallback(api_not_found)
        .layer(middleware::from_fn_with_state(ctx.clone(), require_api_key))
        .with_state(ctx.clone());
    Router::new()
        .nest("/api", api)
        .fallback(get(crate::ui_assets::serve))
}

async fn require_api_key(
    State(ctx): State<ApiContext>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if let Some(expected) = &ctx.api_key {
        let header_key = request
            .headers()
            .get("X-Api-Key")
            .and_then(|v| v.to_str().ok());
        let authorized = if let Some(provided) = header_key {
            provided == expected.as_str()
        } else if request.uri().path() == "/ws" {
            query_param(request.uri().query(), "api_key").as_deref() == Some(expected.as_str())
        } else {
            false
        };
        if !authorized {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    next.run(request).await
}

fn query_param(query: Option<&str>, name: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn api_not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

async fn status(State(ctx): State<ApiContext>) -> Response {
    Json(ctx.host.subscribe().borrow().clone()).into_response()
}

#[derive(Serialize)]
struct VirtualSensorInfo {
    id: Id,
    #[serde(rename = "type")]
    kind: &'static str,
    inputs: Vec<Id>,
}

#[derive(Serialize)]
struct InventoryResponse {
    #[serde(flatten)]
    hardware: Inventory,
    #[serde(rename = "virtual")]
    virtual_sensors: Vec<VirtualSensorInfo>,
}

fn virtual_sensor_infos(config: &GaleConfig) -> Vec<VirtualSensorInfo> {
    let Some(profile) = config.profiles.get(&config.active_profile) else {
        return Vec::new();
    };
    profile
        .sensors
        .iter()
        .map(|(name, cfg)| VirtualSensorInfo {
            id: virtual_id(name),
            kind: cfg.type_name(),
            inputs: cfg.inputs(),
        })
        .collect()
}

async fn inventory(State(ctx): State<ApiContext>) -> Response {
    let hardware = ctx.inventory.read().unwrap().clone();
    let virtual_sensors = virtual_sensor_infos(&ctx.host.config());
    Json(InventoryResponse {
        hardware,
        virtual_sensors,
    })
    .into_response()
}

async fn get_config(State(ctx): State<ApiContext>) -> Response {
    let mut config = ctx.host.config();
    config.api.api_key = None;
    Json(config).into_response()
}

#[derive(Serialize)]
struct WarningsResponse {
    warnings: Vec<String>,
}

async fn get_warnings(State(ctx): State<ApiContext>) -> Response {
    let config = ctx.host.config();
    let warnings = ctx.host.warnings(&config);
    Json(WarningsResponse { warnings }).into_response()
}

fn config_error_response(error: ConfigError) -> Response {
    let status = match error {
        ConfigError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, error.to_string()).into_response()
}

async fn apply_and_persist(ctx: &ApiContext, config: GaleConfig) -> Result<(), ConfigError> {
    ctx.host.replace_config(config.clone()).await?;
    if let Err(error) = ctx.store.save(&config) {
        tracing::warn!(%error, "config persisted to disk failed");
    }
    Ok(())
}

fn merge_api_key(existing: Option<String>, incoming: Option<String>) -> Option<String> {
    match incoming {
        None => existing,
        Some(key) if key.is_empty() => None,
        Some(key) => Some(key),
    }
}

async fn put_config(State(ctx): State<ApiContext>, Json(mut config): Json<GaleConfig>) -> Response {
    let existing_key = ctx.host.config().api.api_key;
    config.api.api_key = merge_api_key(existing_key, config.api.api_key.clone());
    if let Err(error) = validate_profiles(&config) {
        return config_error_response(error);
    }
    match apply_and_persist(&ctx, config.clone()).await {
        Ok(()) => {
            let warnings = ctx.host.config_warnings(&config);
            if warnings.is_empty() {
                StatusCode::NO_CONTENT.into_response()
            } else {
                (StatusCode::OK, Json(WarningsResponse { warnings })).into_response()
            }
        }
        Err(error) => config_error_response(error),
    }
}

async fn activate_profile(State(ctx): State<ApiContext>, Path(name): Path<String>) -> Response {
    let mut config = ctx.host.config();
    if !config.profiles.contains_key(&name) {
        return StatusCode::NOT_FOUND.into_response();
    }
    config.active_profile = name;
    match apply_and_persist(&ctx, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => config_error_response(error),
    }
}

async fn set_control(
    State(ctx): State<ApiContext>,
    Path(id): Path<String>,
    Json(body): Json<DutyBody>,
) -> Response {
    if !body.duty.is_finite() || !(0.0..=100.0).contains(&body.duty) {
        return (
            StatusCode::BAD_REQUEST,
            "duty must be a finite percent in 0..=100",
        )
            .into_response();
    }
    match ctx.host.set_manual(&id, body.duty).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

async fn clear_control(State(ctx): State<ApiContext>, Path(id): Path<String>) -> Response {
    match ctx.host.clear_manual(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

async fn ws_upgrade(State(ctx): State<ApiContext>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| stream_snapshots(socket, ctx))
}

async fn stream_snapshots(mut socket: WebSocket, ctx: ApiContext) {
    let mut receiver = ctx.host.subscribe();
    let initial = receiver.borrow().clone();
    if send_snapshot(&mut socket, &initial).await.is_err() {
        return;
    }
    while receiver.changed().await.is_ok() {
        let snapshot = receiver.borrow_and_update().clone();
        if send_snapshot(&mut socket, &snapshot).await.is_err() {
            return;
        }
    }
}

async fn send_snapshot(
    socket: &mut WebSocket,
    snapshot: &crate::engine_host::Snapshot,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(snapshot).unwrap_or_default();
    socket.send(Message::Text(text)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend_handle::BackendHandle;
    use crate::backend_pool::BackendPool;
    use crate::config_store::ConfigStore;
    use crate::engine_host::EngineHost;
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use gale_core::config::GaleConfig;
    use gale_hw::{Backend, HwError, Id, Inventory};
    use http_body_util::BodyExt;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, RwLock};
    use tower::util::ServiceExt;

    struct NullBackend;

    impl Backend for NullBackend {
        fn name(&self) -> &str {
            "null"
        }
        fn enumerate(&mut self) -> Result<Inventory, HwError> {
            Ok(Inventory::default())
        }
        fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
            [("t1".to_string(), Some(40.0))].into()
        }
        fn set_duty(&mut self, _id: &str, _pct: f64) -> Result<(), HwError> {
            Ok(())
        }
        fn release(&mut self, _id: &str) -> Result<(), HwError> {
            Ok(())
        }
    }

    const CONFIG: &str = r#"
active_profile = "p"

[profiles.p.curves.flat]
type = "flat"
duty = 42.0

[profiles.quiet.curves.flat]
type = "flat"
duty = 10.0

[profiles.p.assignments]
"pwm1" = "flat"
"#;

    const CONFIG_VIRTUAL: &str = r#"
active_profile = "p"

[profiles.p.curves.flat]
type = "flat"
duty = 42.0

[profiles.quiet.curves.flat]
type = "flat"
duty = 10.0

[profiles.p.sensors.cpu_hot]
type = "max"
inputs = ["t1", "t2"]

[profiles.p.curves.hot]
type = "point"
sensor = "virtual/cpu_hot"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.p.assignments]
"pwm1" = "flat"
"pwm2" = "hot"
"#;

    fn make_router(api_key: Option<String>) -> (axum::Router, Arc<EngineHost>) {
        make_router_with(CONFIG, api_key)
    }

    fn make_router_with(
        config_toml: &str,
        api_key: Option<String>,
    ) -> (axum::Router, Arc<EngineHost>) {
        let handle = BackendHandle::spawn(Box::new(NullBackend));
        let pool = BackendPool::new(vec![handle]);
        let host = EngineHost::new(GaleConfig::from_toml(config_toml).unwrap(), pool).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::new(dir.path().join("config.toml")));
        std::mem::forget(dir);
        let ctx = ApiContext {
            host: host.clone(),
            store,
            inventory: Arc::new(RwLock::new(Inventory::default())),
            api_key,
        };
        (router(ctx), host)
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn status_reflects_last_tick() {
        let (router, host) = make_router(None);
        host.tick(1.0).await;
        let response = router
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["duties"]["pwm1"], 42.0);
        assert_eq!(json["sensors"]["t1"], 40.0);
    }

    #[tokio::test]
    async fn api_key_gate_rejects_missing_and_accepts_matching() {
        let (router, _host) = make_router(Some("secret".into()));
        let denied = router
            .clone()
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        let allowed = router
            .oneshot(
                Request::get("/api/status")
                    .header("X-Api-Key", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn status_reports_overrides_and_clears_them() {
        let (router, host) = make_router(None);
        let put = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/controls/pwm1")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"duty": 55.0}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::NO_CONTENT);
        host.tick(1.0).await;
        let response = router
            .clone()
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let json = body_json(response).await;
        assert_eq!(json["overrides"], serde_json::json!(["pwm1"]));

        let delete = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/controls/pwm1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete.status(), StatusCode::NO_CONTENT);
        host.tick(1.0).await;
        let response = router
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let json = body_json(response).await;
        assert_eq!(json["overrides"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn manual_override_endpoints_validate_and_route_slashed_ids() {
        let (router, host) = make_router(None);
        let put = |uri: &str, body: &str| {
            Request::builder()
                .method(Method::PUT)
                .uri(uri.to_string())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let ok = router
            .clone()
            .oneshot(put("/api/controls/hwmon/nct6798/pwm1", r#"{"duty": 55.0}"#))
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.subscribe().borrow().manual.len(), 0);
        host.tick(1.0).await;
        assert_eq!(host.subscribe().borrow().manual["hwmon/nct6798/pwm1"], 55.0);
        let bad = router
            .clone()
            .oneshot(put("/api/controls/pwm1", r#"{"duty": 150.0}"#))
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        let delete = router
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/controls/hwmon/nct6798/pwm1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn config_put_validates_and_profile_activation_switches() {
        let (router, host) = make_router(None);
        let mut bad = GaleConfig::from_toml(CONFIG).unwrap();
        bad.active_profile = "ghost".into();
        let rejected = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&bad).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let activated = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/profiles/quiet/activate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(activated.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.config().active_profile, "quiet");
        let missing = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/profiles/ghost/activate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn config_put_returns_warnings_for_ghost_sensor_and_204_for_clean() {
        let (router, host) = make_router(None);
        host.set_known_sensors(["t1".to_string()].into());

        let mut with_ghost = GaleConfig::from_toml(CONFIG).unwrap();
        let profile = with_ghost.profiles.get_mut("p").unwrap();
        profile.curves.insert(
            "cpu".to_string(),
            gale_core::config::CurveConfig::Point {
                sensor: "ghost".to_string(),
                points: vec![[30.0, 20.0], [70.0, 100.0]],
                hysteresis: None,
                response: None,
            },
        );
        profile
            .assignments
            .insert("pwm2".to_string(), "cpu".to_string());
        let warned = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&with_ghost).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(warned.status(), StatusCode::OK);
        let json = body_json(warned).await;
        assert_eq!(json["warnings"].as_array().unwrap().len(), 1);
        assert!(json["warnings"][0].as_str().unwrap().contains("ghost"));

        let clean = GaleConfig::from_toml(CONFIG).unwrap();
        let ok = router
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&clean).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn ws_query_param_api_key_passes_auth_gate() {
        let (router, _host) = make_router(Some("secret".into()));
        let response = router
            .oneshot(
                Request::get("/api/ws?api_key=secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ws_wrong_query_param_api_key_is_unauthorized() {
        let (router, _host) = make_router(Some("secret".into()));
        let response = router
            .oneshot(
                Request::get("/api/ws?api_key=wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_query_param_api_key_is_still_unauthorized() {
        let (router, _host) = make_router(Some("secret".into()));
        let response = router
            .oneshot(
                Request::get("/api/status?api_key=secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn root_serves_ui_index_html() {
        let (router, _host) = make_router(None);
        let response = router
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(content_type.contains("text/html"));
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.to_lowercase().contains("gale"));
    }

    #[tokio::test]
    async fn real_asset_served_with_correct_content_type() {
        let (router, _host) = make_router(None);
        let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist/assets");
        let js_file = std::fs::read_dir(dist)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext == "js")
                    .unwrap_or(false)
            })
            .unwrap()
            .file_name()
            .to_string_lossy()
            .to_string();
        let uri = format!("/assets/{js_file}");
        let response = router
            .oneshot(Request::get(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(content_type.contains("javascript"));
    }

    #[tokio::test]
    async fn spa_route_falls_back_to_index_html() {
        let (router, _host) = make_router(None);
        let response = router
            .oneshot(Request::get("/some/spa/route").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(content_type.contains("text/html"));
    }

    #[tokio::test]
    async fn unknown_api_path_returns_404_not_html() {
        let (router, _host) = make_router(None);
        let response = router
            .oneshot(
                Request::get("/api/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        assert!(!content_type.contains("text/html"));
    }

    #[tokio::test]
    async fn get_warnings_reports_current_config_warnings() {
        let (router, host) = make_router(None);
        host.set_known_sensors(HashSet::new());
        let response = router
            .clone()
            .oneshot(Request::get("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["warnings"].as_array().unwrap().len(), 0);

        host.set_known_sensors(["other".to_string()].into());
        let mut config = host.config();
        let profile = config.profiles.get_mut("p").unwrap();
        profile.curves.insert(
            "cpu".to_string(),
            gale_core::config::CurveConfig::Point {
                sensor: "ghost".to_string(),
                points: vec![[30.0, 20.0], [70.0, 100.0]],
                hysteresis: None,
                response: None,
            },
        );
        profile
            .assignments
            .insert("pwm2".to_string(), "cpu".to_string());
        host.replace_config(config).await.unwrap();
        let response = router
            .oneshot(Request::get("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["warnings"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_warnings_includes_platform_warning_and_put_config_still_returns_204() {
        let (router, host) = make_router(None);
        host.set_platform_warnings(vec!["a https://pawnio.eu".to_string()]);
        let response = router
            .clone()
            .oneshot(Request::get("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        let warnings = json["warnings"].as_array().unwrap();
        assert!(warnings
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("pawnio.eu")));

        let clean = GaleConfig::from_toml(CONFIG).unwrap();
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&clean).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn get_config_always_redacts_api_key() {
        let (router, host) = make_router(None);
        let mut config = host.config();
        config.api.api_key = Some("realkey".to_string());
        host.replace_config(config).await.unwrap();
        let response = router
            .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert!(json["api"]["api_key"].is_null());
    }

    #[tokio::test]
    async fn put_config_with_null_api_key_preserves_existing_key() {
        let (router, host) = make_router(None);
        let mut config = host.config();
        config.api.api_key = Some("realkey".to_string());
        host.replace_config(config).await.unwrap();

        let mut incoming = GaleConfig::from_toml(CONFIG).unwrap();
        incoming.api.api_key = None;
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&incoming).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.config().api.api_key, Some("realkey".to_string()));
    }

    #[tokio::test]
    async fn put_config_with_empty_string_api_key_clears_it() {
        let (router, host) = make_router(None);
        let mut config = host.config();
        config.api.api_key = Some("realkey".to_string());
        host.replace_config(config).await.unwrap();

        let mut incoming = GaleConfig::from_toml(CONFIG).unwrap();
        incoming.api.api_key = Some(String::new());
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&incoming).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.config().api.api_key, None);
    }

    #[tokio::test]
    async fn put_config_with_string_api_key_sets_it_and_persists_real_value() {
        let handle = BackendHandle::spawn(Box::new(NullBackend));
        let pool = BackendPool::new(vec![handle]);
        let host = EngineHost::new(GaleConfig::from_toml(CONFIG).unwrap(), pool).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::new(dir.path().join("config.toml")));
        let ctx = ApiContext {
            host: host.clone(),
            store: store.clone(),
            inventory: Arc::new(RwLock::new(Inventory::default())),
            api_key: None,
        };
        let router = router(ctx);

        let mut incoming = GaleConfig::from_toml(CONFIG).unwrap();
        incoming.api.api_key = Some("newkey".to_string());
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&incoming).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.config().api.api_key, Some("newkey".to_string()));

        let persisted = store.load().unwrap();
        assert_eq!(persisted.api.api_key, Some("newkey".to_string()));
    }

    #[tokio::test]
    async fn unknown_path_with_api_key_configured_is_unauthorized_before_404() {
        let (router, _host) = make_router(Some("secret".into()));
        let response = router
            .oneshot(
                Request::get("/api/nonexistent-with-key-configured")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_includes_virtual_sensor_values() {
        let (router, host) = make_router_with(CONFIG_VIRTUAL, None);
        host.tick(1.0).await;
        let response = router
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["sensors"]["virtual/cpu_hot"], 40.0);
        assert_eq!(json["duties"]["pwm2"], 40.0);
    }

    #[tokio::test]
    async fn inventory_lists_virtual_sensors_of_active_profile() {
        let (router, _host) = make_router_with(CONFIG_VIRTUAL, None);
        let response = router
            .oneshot(Request::get("/api/inventory").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(
            json["virtual"],
            serde_json::json!([{"id": "virtual/cpu_hot", "type": "max", "inputs": ["t1", "t2"]}])
        );
        assert!(json["sensors"].is_array());
        assert!(json["controls"].is_array());
    }

    #[tokio::test]
    async fn inventory_virtual_list_follows_active_profile() {
        let (router, _host) = make_router_with(CONFIG_VIRTUAL, None);
        let activated = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/profiles/quiet/activate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(activated.status(), StatusCode::NO_CONTENT);
        let response = router
            .oneshot(Request::get("/api/inventory").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["virtual"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn config_put_rejects_virtual_cycle_with_422() {
        let (router, _host) = make_router(None);
        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        let profile = config.profiles.get_mut("p").unwrap();
        profile.sensors.insert(
            "a".to_string(),
            gale_core::config::VirtualSensorConfig::Max {
                inputs: vec!["virtual/b".to_string()],
            },
        );
        profile.sensors.insert(
            "b".to_string(),
            gale_core::config::VirtualSensorConfig::Min {
                inputs: vec!["virtual/a".to_string()],
            },
        );
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&config).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains("cycle"));
    }

    fn put_config_request(config: &GaleConfig) -> Request<Body> {
        Request::builder()
            .method(Method::PUT)
            .uri("/api/config")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_string(config).unwrap()))
            .unwrap()
    }

    async fn body_text(response: axum::response::Response) -> String {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn config_put_rejects_cycle_in_inactive_profile_with_422() {
        let (router, host) = make_router(None);
        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        let quiet = config.profiles.get_mut("quiet").unwrap();
        quiet.sensors.insert(
            "a".to_string(),
            gale_core::config::VirtualSensorConfig::Max {
                inputs: vec!["virtual/b".to_string()],
            },
        );
        quiet.sensors.insert(
            "b".to_string(),
            gale_core::config::VirtualSensorConfig::Min {
                inputs: vec!["virtual/a".to_string()],
            },
        );
        let response = router.oneshot(put_config_request(&config)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_text(response).await;
        assert!(body.contains("profile 'quiet'"), "{body}");
        assert!(body.contains("cycle"), "{body}");
        assert!(host.config().profiles["quiet"].sensors.is_empty());
    }

    #[tokio::test]
    async fn config_put_rejects_undefined_virtual_reference_in_inactive_profile() {
        let (router, _host) = make_router(None);
        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        let quiet = config.profiles.get_mut("quiet").unwrap();
        quiet.sensors.insert(
            "a".to_string(),
            gale_core::config::VirtualSensorConfig::Max {
                inputs: vec!["virtual/ghost".to_string()],
            },
        );
        let response = router
            .clone()
            .oneshot(put_config_request(&config))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_text(response).await;
        assert!(body.contains("profile 'quiet'"), "{body}");
        assert!(body.contains("virtual/ghost"), "{body}");

        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        let quiet = config.profiles.get_mut("quiet").unwrap();
        quiet.curves.insert(
            "hot".to_string(),
            gale_core::config::CurveConfig::Point {
                sensor: "virtual/ghost".to_string(),
                points: vec![[30.0, 20.0], [70.0, 100.0]],
                hysteresis: None,
                response: None,
            },
        );
        let response = router.oneshot(put_config_request(&config)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_text(response).await;
        assert!(body.contains("profile 'quiet'"), "{body}");
        assert!(body.contains("virtual/ghost"), "{body}");
    }

    #[tokio::test]
    async fn config_put_rejects_dangling_curve_reference_in_inactive_profile() {
        let (router, _host) = make_router(None);
        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        let quiet = config.profiles.get_mut("quiet").unwrap();
        quiet.curves.insert(
            "mirror".to_string(),
            gale_core::config::CurveConfig::Sync {
                source: "missing".to_string(),
            },
        );
        quiet
            .assignments
            .insert("pwm1".to_string(), "ghost".to_string());
        let response = router.oneshot(put_config_request(&config)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_text(response).await;
        assert!(body.contains("profile 'quiet'"), "{body}");
        assert!(body.contains("missing"), "{body}");
    }

    #[tokio::test]
    async fn config_put_accepts_valid_virtual_sensors_in_inactive_profile() {
        let (router, host) = make_router(None);
        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        let quiet = config.profiles.get_mut("quiet").unwrap();
        quiet.sensors.insert(
            "hot".to_string(),
            gale_core::config::VirtualSensorConfig::Max {
                inputs: vec!["t1".to_string()],
            },
        );
        quiet.curves.insert(
            "cpu".to_string(),
            gale_core::config::CurveConfig::Point {
                sensor: "virtual/hot".to_string(),
                points: vec![[30.0, 20.0], [70.0, 100.0]],
                hysteresis: None,
                response: None,
            },
        );
        quiet
            .assignments
            .insert("pwm1".to_string(), "cpu".to_string());
        let response = router.oneshot(put_config_request(&config)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.config().profiles["quiet"].sensors.len(), 1);
    }

    #[tokio::test]
    async fn config_put_warns_for_hardware_input_behind_virtual_sensor() {
        let (router, host) = make_router(None);
        host.set_known_sensors(["t1".to_string()].into());
        let config = GaleConfig::from_toml(CONFIG_VIRTUAL).unwrap();
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/api/config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&config).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        let warnings = json["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].as_str().unwrap().contains("t2"));
        assert!(!warnings[0].as_str().unwrap().contains("virtual"));
    }

    #[test]
    fn query_param_decodes_encoded_space_and_plus() {
        assert_eq!(
            query_param(Some("api_key=sek%20ret%2B1"), "api_key").as_deref(),
            Some("sek ret+1")
        );
        assert_eq!(
            query_param(Some("api_key=sek+ret%2B1"), "api_key").as_deref(),
            Some("sek ret+1")
        );
    }
}
