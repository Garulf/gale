use crate::config_store::ConfigStore;
use crate::engine_host::EngineHost;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use gale_core::build::validate_profiles;
use gale_core::config::{virtual_id, ConfigError, DashboardUiConfig, GaleConfig};
use gale_core::presets::{self, CurvePreset};
use gale_hw::{Id, Inventory};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use subtle::ConstantTimeEq;

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
        .route("/labels/*id", put(put_label).delete(delete_label))
        .route("/config", get(get_config).put(put_config))
        .route("/config.toml", get(get_config_toml))
        .route("/ui/dashboard", put(put_dashboard_ui))
        .route("/presets", get(get_presets))
        .route("/presets/:name", put(put_preset).delete(delete_preset))
        .route("/warnings", get(get_warnings))
        .route("/profiles/:name/activate", post(activate_profile))
        .route("/controls/*id", put(set_control).delete(clear_control))
        .route("/webhook/:token", post(post_webhook))
        .route("/webhook-url/:name", get(get_webhook_url))
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
        if request.uri().path().starts_with("/webhook/") {
            return next.run(request).await;
        }
        let header_key = request
            .headers()
            .get("X-Api-Key")
            .and_then(|v| v.to_str().ok());
        let authorized = if let Some(provided) = header_key {
            bool::from(provided.as_bytes().ct_eq(expected.as_bytes()))
        } else if request.uri().path() == "/ws" {
            query_param(request.uri().query(), "api_key")
                .map(|provided| bool::from(provided.as_bytes().ct_eq(expected.as_bytes())))
                .unwrap_or(false)
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

fn apply_labels(inventory: &mut Inventory, labels: &BTreeMap<Id, String>) {
    for sensor in &mut inventory.sensors {
        if let Some(label) = labels.get(&sensor.id) {
            sensor.label = label.clone();
        }
    }
    for control in &mut inventory.controls {
        if let Some(label) = labels.get(&control.id) {
            control.label = label.clone();
        }
    }
}

async fn inventory(State(ctx): State<ApiContext>) -> Response {
    let config = ctx.host.config();
    let mut hardware = ctx.inventory.read().unwrap().clone();
    apply_labels(&mut hardware, &config.labels);
    let virtual_sensors = virtual_sensor_infos(&config);
    Json(InventoryResponse {
        hardware,
        virtual_sensors,
    })
    .into_response()
}

fn redact_webhook_tokens(config: &mut GaleConfig) {
    for profile in config.profiles.values_mut() {
        for sensor in profile.sensors.values_mut() {
            if let gale_core::config::VirtualSensorConfig::Webhook { token, .. } = sensor {
                token.clear();
            }
        }
    }
}

fn redacted_config(ctx: &ApiContext) -> GaleConfig {
    let mut config = ctx.host.config();
    config.api.api_key = None;
    redact_webhook_tokens(&mut config);
    config
}

async fn get_config(State(ctx): State<ApiContext>) -> Response {
    Json(redacted_config(&ctx)).into_response()
}

async fn get_config_toml(State(ctx): State<ApiContext>) -> Response {
    match redacted_config(&ctx).to_toml() {
        Ok(toml) => ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], toml).into_response(),
        Err(error) => config_error_response(error),
    }
}

#[derive(Deserialize)]
struct LabelBody {
    label: String,
}

async fn put_label(
    State(ctx): State<ApiContext>,
    Path(id): Path<String>,
    Json(body): Json<LabelBody>,
) -> Response {
    let label = body.label.trim().to_string();
    if label.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "label must not be empty; delete the label to restore the hardware name",
        )
            .into_response();
    }
    let mut config = ctx.host.config();
    config.labels.insert(id, label);
    match apply_and_persist(&ctx, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => config_error_response(error),
    }
}

async fn delete_label(State(ctx): State<ApiContext>, Path(id): Path<String>) -> Response {
    let mut config = ctx.host.config();
    if config.labels.remove(&id).is_none() {
        return (StatusCode::NOT_FOUND, format!("no label set for '{id}'")).into_response();
    }
    match apply_and_persist(&ctx, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => config_error_response(error),
    }
}

async fn put_dashboard_ui(
    State(ctx): State<ApiContext>,
    Json(dashboard): Json<DashboardUiConfig>,
) -> Response {
    let mut config = ctx.host.config();
    config.ui.dashboard = dashboard;
    match apply_and_persist(&ctx, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => config_error_response(error),
    }
}

#[derive(Serialize)]
struct PresetsResponse {
    builtin: BTreeMap<String, CurvePreset>,
    user: BTreeMap<String, CurvePreset>,
}

async fn get_presets(State(ctx): State<ApiContext>) -> Response {
    Json(PresetsResponse {
        builtin: presets::builtin(),
        user: ctx.host.config().presets,
    })
    .into_response()
}

async fn put_preset(
    State(ctx): State<ApiContext>,
    Path(name): Path<String>,
    Json(preset): Json<CurvePreset>,
) -> Response {
    if let Err(error) = presets::validate_preset(&name, &preset) {
        return config_error_response(error);
    }
    let mut config = ctx.host.config();
    config.presets.insert(name, preset);
    match apply_and_persist(&ctx, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => config_error_response(error),
    }
}

async fn delete_preset(State(ctx): State<ApiContext>, Path(name): Path<String>) -> Response {
    if presets::is_builtin_name(&name) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("preset '{name}' is built in and cannot be deleted"),
        )
            .into_response();
    }
    let mut config = ctx.host.config();
    if config.presets.remove(&name).is_none() {
        return (
            StatusCode::NOT_FOUND,
            format!("preset '{name}' does not exist"),
        )
            .into_response();
    }
    match apply_and_persist(&ctx, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => config_error_response(error),
    }
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
    let existing = ctx.host.config();
    config.api.api_key = merge_api_key(existing.api.api_key.clone(), config.api.api_key.clone());
    crate::webhooks::fill_missing_tokens(&mut config, &existing);
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

fn webhook_value(body: &[u8]) -> Option<f64> {
    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    match json.get("value")? {
        serde_json::Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        serde_json::Value::Number(number) => number.as_f64().filter(|value| value.is_finite()),
        _ => None,
    }
}

async fn post_webhook(
    State(ctx): State<ApiContext>,
    Path(token): Path<String>,
    body: Bytes,
) -> Response {
    let Some(value) = webhook_value(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            "body must be a JSON object with a finite number or boolean \"value\"",
        )
            .into_response();
    };
    if ctx.host.record_webhook(&token, value) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

#[derive(Serialize)]
struct WebhookUrlResponse {
    url: String,
}

async fn get_webhook_url(
    State(ctx): State<ApiContext>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = ctx.host.webhook_token(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| ctx.host.config().api.bind.clone());
    Json(WebhookUrlResponse {
        url: format!("http://{host}/api/webhook/{token}"),
    })
    .into_response()
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
    async fn api_key_gate_rejects_wrong_key_under_constant_time_comparison() {
        let (router, _host) = make_router(Some("secret".into()));
        let wrong = router
            .clone()
            .oneshot(
                Request::get("/api/status")
                    .header("X-Api-Key", "secrer")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
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

    fn make_router_with_inventory(inventory: Inventory) -> (axum::Router, Arc<EngineHost>) {
        let handle = BackendHandle::spawn(Box::new(NullBackend));
        let pool = BackendPool::new(vec![handle]);
        let host = EngineHost::new(GaleConfig::from_toml(CONFIG).unwrap(), pool).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::new(dir.path().join("config.toml")));
        std::mem::forget(dir);
        let ctx = ApiContext {
            host: host.clone(),
            store,
            inventory: Arc::new(RwLock::new(inventory)),
            api_key: None,
        };
        (router(ctx), host)
    }

    fn label_request(method: &str, id: &str, body: Option<&str>) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(format!("/api/labels/{id}"))
            .header("content-type", "application/json")
            .body(Body::from(body.unwrap_or("").to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn labels_override_inventory_names_and_delete_restores_them() {
        let inventory = Inventory {
            sensors: vec![gale_hw::SensorInfo {
                id: "hwmon/x/temp1".to_string(),
                label: "CPUTIN".to_string(),
                kind: gale_hw::SensorKind::Temp,
            }],
            controls: vec![gale_hw::ControlInfo {
                id: "hwmon/x/pwm1".to_string(),
                label: "pwm1".to_string(),
            }],
        };
        let (router, host) = make_router_with_inventory(inventory);
        let response = router
            .clone()
            .oneshot(label_request(
                "PUT",
                "hwmon/x/temp1",
                Some(r#"{"label":" CPU die "}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = router
            .clone()
            .oneshot(label_request(
                "PUT",
                "hwmon/x/pwm1",
                Some(r#"{"label":"Front intake"}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.config().labels["hwmon/x/temp1"], "CPU die");

        let json = body_json(
            router
                .clone()
                .oneshot(Request::get("/api/inventory").body(Body::empty()).unwrap())
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(json["sensors"][0]["label"], "CPU die");
        assert_eq!(json["controls"][0]["label"], "Front intake");

        let response = router
            .clone()
            .oneshot(label_request("DELETE", "hwmon/x/temp1", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let json = body_json(
            router
                .clone()
                .oneshot(Request::get("/api/inventory").body(Body::empty()).unwrap())
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(json["sensors"][0]["label"], "CPUTIN");

        let response = router
            .clone()
            .oneshot(label_request("DELETE", "hwmon/x/temp1", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = router
            .oneshot(label_request(
                "PUT",
                "hwmon/x/temp1",
                Some(r#"{"label":"   "}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn dashboard_hidden_list_persists_and_echoes_in_config() {
        let (router, host) = make_router(None);
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/ui/dashboard")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"hidden":["hwmon/x/temp3","hwmon/x/pwm2"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            host.config().ui.dashboard.hidden,
            vec!["hwmon/x/temp3".to_string(), "hwmon/x/pwm2".to_string()]
        );
        let json = body_json(
            router
                .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(json["ui"]["dashboard"]["hidden"][1], "hwmon/x/pwm2");
    }

    fn preset_request(method: &str, name: &str, body: Option<&str>) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(format!("/api/presets/{name}"))
            .header("content-type", "application/json")
            .body(Body::from(body.unwrap_or("").to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn get_presets_lists_builtins_and_empty_user_map() {
        let (router, _host) = make_router(None);
        let response = router
            .oneshot(Request::get("/api/presets").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["builtin"]["Quiet"]["type"], "point");
        assert_eq!(json["builtin"].as_object().unwrap().len(), 3);
        assert_eq!(json["user"].as_object().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn put_preset_creates_overwrites_and_delete_removes() {
        let (router, host) = make_router(None);
        let response = router
            .clone()
            .oneshot(preset_request(
                "PUT",
                "mine",
                Some(r#"{"type":"flat","duty":40}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            matches!(host.config().presets["mine"], CurvePreset::Flat { duty } if duty == 40.0)
        );

        let response = router
            .clone()
            .oneshot(preset_request(
                "PUT",
                "mine",
                Some(r#"{"type":"flat","duty":55}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            matches!(host.config().presets["mine"], CurvePreset::Flat { duty } if duty == 55.0)
        );

        let json = body_json(
            router
                .clone()
                .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(json["presets"]["mine"]["duty"], 55.0);

        let response = router
            .clone()
            .oneshot(preset_request("DELETE", "mine", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(host.config().presets.is_empty());

        let response = router
            .oneshot(preset_request("DELETE", "mine", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn preset_endpoints_protect_builtin_names_and_reject_bad_shapes() {
        let (router, host) = make_router(None);
        let response = router
            .clone()
            .oneshot(preset_request(
                "PUT",
                "quiet",
                Some(r#"{"type":"flat","duty":40}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = router
            .clone()
            .oneshot(preset_request("DELETE", "Quiet", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = router
            .oneshot(preset_request(
                "PUT",
                "bad",
                Some(r#"{"type":"point","points":[]}"#),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(host.config().presets.is_empty());
    }

    #[tokio::test]
    async fn get_config_toml_serves_the_daemon_serializer_with_secrets_redacted() {
        let (router, host) = make_router(None);
        let mut config = host.config();
        config.api.api_key = Some("realkey".to_string());
        host.replace_config(config).await.unwrap();
        let response = router
            .oneshot(
                Request::get("/api/config.toml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
        let body = body_text(response).await;
        assert!(body.contains("active_profile = \"p\""), "{body}");
        assert!(!body.contains("realkey"), "{body}");
        assert_eq!(GaleConfig::from_toml(&body).unwrap().api.api_key, None);
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

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER_TOKEN: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    const CONFIG_WEBHOOK: &str = r#"
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

[profiles.p.assignments]
"pwm1" = "flat"
"pwm2" = "remote"
"#;

    fn webhook_config() -> String {
        CONFIG_WEBHOOK
            .replace("{TOKEN}", TOKEN)
            .replace("{OTHER_TOKEN}", OTHER_TOKEN)
    }

    fn post_webhook_request(token: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/webhook/{token}"))
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn status_json(router: &axum::Router, host: &EngineHost) -> serde_json::Value {
        host.tick(1.0).await;
        let response = router
            .clone()
            .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        body_json(response).await
    }

    #[tokio::test]
    async fn webhook_post_returns_204_and_status_reflects_the_value() {
        let (router, host) = make_router_with(&webhook_config(), None);
        let json = status_json(&router, &host).await;
        assert!(json["sensors"]["virtual/hook"].is_null());
        assert_eq!(json["duties"]["pwm2"], 100.0);

        let response = router
            .clone()
            .oneshot(post_webhook_request(TOKEN, r#"{"value": 51.5}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let json = status_json(&router, &host).await;
        assert_eq!(json["sensors"]["virtual/hook"], 51.5);
        assert_eq!(json["duties"]["pwm2"], 63.0);
    }

    #[tokio::test]
    async fn webhook_post_maps_booleans_to_one_and_zero() {
        let (router, host) = make_router_with(&webhook_config(), None);
        let response = router
            .clone()
            .oneshot(post_webhook_request(OTHER_TOKEN, r#"{"value": true}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let json = status_json(&router, &host).await;
        assert_eq!(json["sensors"]["virtual/forever"], 1.0);

        let response = router
            .clone()
            .oneshot(post_webhook_request(OTHER_TOKEN, r#"{"value": false}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let json = status_json(&router, &host).await;
        assert_eq!(json["sensors"]["virtual/forever"], 0.0);
    }

    #[tokio::test]
    async fn webhook_post_unknown_token_is_404() {
        let (router, _host) = make_router_with(&webhook_config(), None);
        let response = router
            .clone()
            .oneshot(post_webhook_request(&"f".repeat(64), r#"{"value": 1.0}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = router
            .oneshot(post_webhook_request("short", r#"{"value": 1.0}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn webhook_post_rejects_malformed_bodies_with_400() {
        let (router, host) = make_router_with(&webhook_config(), None);
        for body in [
            r#"{"value": "41"}"#,
            r#"{"value": null}"#,
            "[41]",
            "41",
            "not json",
            "{}",
            r#"{"value": 1e999}"#,
        ] {
            let response = router
                .clone()
                .oneshot(post_webhook_request(TOKEN, body))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
            let response = router
                .clone()
                .oneshot(post_webhook_request(&"f".repeat(64), body))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        }
        let json = status_json(&router, &host).await;
        assert!(json["sensors"]["virtual/hook"].is_null());
    }

    #[tokio::test]
    async fn webhook_post_bypasses_api_key_but_webhook_url_does_not() {
        let (router, _host) = make_router_with(&webhook_config(), Some("secret".into()));
        let response = router
            .clone()
            .oneshot(post_webhook_request(TOKEN, r#"{"value": 2.0}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let denied = router
            .clone()
            .oneshot(
                Request::get("/api/webhook-url/hook")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        let allowed = router
            .oneshot(
                Request::get("/api/webhook-url/hook")
                    .header("X-Api-Key", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn webhook_exemption_never_reaches_other_endpoints() {
        let (router, _host) = make_router_with(&webhook_config(), Some("secret".into()));
        for (uri, expected) in [
            ("/api/webhook/../status".to_string(), StatusCode::NOT_FOUND),
            (
                format!("/api/webhook/{TOKEN}/../status"),
                StatusCode::NOT_FOUND,
            ),
            (
                "/api/webhook/..%2Fstatus".to_string(),
                StatusCode::METHOD_NOT_ALLOWED,
            ),
            ("/api/webhook".to_string(), StatusCode::UNAUTHORIZED),
            ("/api/webhook/".to_string(), StatusCode::NOT_FOUND),
            ("/api/webhookstatus".to_string(), StatusCode::UNAUTHORIZED),
            (
                "/api/webhook-url/hook".to_string(),
                StatusCode::UNAUTHORIZED,
            ),
            ("/api/status".to_string(), StatusCode::UNAUTHORIZED),
        ] {
            let response = router
                .clone()
                .oneshot(Request::get(&uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{uri}");
        }
    }

    #[tokio::test]
    async fn webhook_url_uses_host_header_and_falls_back_to_bind() {
        let (router, _host) = make_router_with(&webhook_config(), None);
        let response = router
            .clone()
            .oneshot(
                Request::get("/api/webhook-url/hook")
                    .header(header::HOST, "gale.local:5250")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(
            json["url"],
            format!("http://gale.local:5250/api/webhook/{TOKEN}")
        );

        let response = router
            .oneshot(
                Request::get("/api/webhook-url/hook")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(
            json["url"],
            format!("http://127.0.0.1:5250/api/webhook/{TOKEN}")
        );
    }

    #[tokio::test]
    async fn webhook_url_is_404_for_non_webhook_and_unknown_names() {
        let (router, _host) = make_router_with(&webhook_config(), None);
        for name in ["cpu_hot", "ghost"] {
            let response = router
                .clone()
                .oneshot(
                    Request::get(format!("/api/webhook-url/{name}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{name}");
        }
    }

    #[tokio::test]
    async fn webhook_value_goes_stale_between_ticks_without_a_new_post() {
        let (router, host) = make_router_with(&webhook_config(), None);
        let response = router
            .clone()
            .oneshot(post_webhook_request(TOKEN, r#"{"value": 51.5}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = router
            .clone()
            .oneshot(post_webhook_request(OTHER_TOKEN, r#"{"value": 30.0}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let json = status_json(&router, &host).await;
        assert_eq!(json["sensors"]["virtual/hook"], 51.5);

        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let json = status_json(&router, &host).await;
        assert!(json["sensors"]["virtual/hook"].is_null());
        assert_eq!(json["duties"]["pwm2"], 100.0);
        assert_eq!(json["sensors"]["virtual/forever"], 30.0);
    }

    #[tokio::test]
    async fn inventory_lists_webhook_sensor_with_empty_inputs() {
        let (router, _host) = make_router_with(&webhook_config(), None);
        let response = router
            .oneshot(Request::get("/api/inventory").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(
            json["virtual"],
            serde_json::json!([
                {"id": "virtual/cpu_hot", "type": "max", "inputs": ["t1", "t2"]},
                {"id": "virtual/forever", "type": "webhook", "inputs": []},
                {"id": "virtual/hook", "type": "webhook", "inputs": []}
            ])
        );
    }

    fn webhook_token_of(config: &GaleConfig, name: &str) -> String {
        match &config.profiles["p"].sensors[name] {
            gale_core::config::VirtualSensorConfig::Webhook { token, .. } => token.clone(),
            other => panic!("expected webhook, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn config_put_generates_tokens_for_empty_webhook_tokens_and_reuses_on_resave() {
        let (router, host) = make_router(None);
        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        config.profiles.get_mut("p").unwrap().sensors.insert(
            "fresh".to_string(),
            gale_core::config::VirtualSensorConfig::Webhook {
                token: String::new(),
                timeout_s: None,
            },
        );
        let response = router
            .clone()
            .oneshot(put_config_request(&config))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let first = webhook_token_of(&host.config(), "fresh");
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));

        let response = router
            .clone()
            .oneshot(put_config_request(&config))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(webhook_token_of(&host.config(), "fresh"), first);

        let response = router
            .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let json = body_json(response).await;
        let fresh = &json["profiles"]["p"]["sensors"]["fresh"];
        assert_eq!(fresh["token"], "");
        assert!(fresh.get("timeout_s").is_none());
    }

    #[tokio::test]
    async fn config_put_rejects_non_positive_timeout_with_422() {
        let (router, _host) = make_router(None);
        let mut config = GaleConfig::from_toml(CONFIG).unwrap();
        config.profiles.get_mut("p").unwrap().sensors.insert(
            "bad".to_string(),
            gale_core::config::VirtualSensorConfig::Webhook {
                token: TOKEN.to_string(),
                timeout_s: Some(0.0),
            },
        );
        let response = router.oneshot(put_config_request(&config)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_text(response).await;
        assert!(
            body.contains("virtual sensor 'bad' timeout_s must be a positive number"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn config_put_renaming_a_webhook_keeps_the_token_it_carries() {
        let (router, host) = make_router_with(&webhook_config(), None);
        let mut config = host.config();
        let profile = config.profiles.get_mut("p").unwrap();
        profile.sensors.remove("hook").unwrap();
        profile.sensors.insert(
            "hook2".to_string(),
            gale_core::config::VirtualSensorConfig::Webhook {
                token: TOKEN.to_string(),
                timeout_s: Some(0.05),
            },
        );
        match profile.curves.get_mut("remote").unwrap() {
            gale_core::config::CurveConfig::Point { sensor, .. } => {
                *sensor = "virtual/hook2".to_string();
            }
            other => panic!("expected point curve, got {other:?}"),
        }
        let response = router.oneshot(put_config_request(&config)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.webhook_token("hook2"), Some(TOKEN.to_string()));
        assert_eq!(host.webhook_token("hook"), None);
    }

    #[tokio::test]
    async fn get_config_redacts_webhook_tokens_but_the_real_token_still_works() {
        let (router, _host) = make_router_with(&webhook_config(), None);
        let response = router
            .clone()
            .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["profiles"]["p"]["sensors"]["hook"]["token"], "");
        assert_eq!(json["profiles"]["p"]["sensors"]["forever"]["token"], "");

        let response = router
            .oneshot(post_webhook_request(TOKEN, r#"{"value": 51.5}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn put_config_with_redacted_webhook_token_round_trips_without_wiping_it() {
        let (router, host) = make_router_with(&webhook_config(), None);
        let response = router
            .clone()
            .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let redacted: GaleConfig = {
            let json = body_json(response).await;
            serde_json::from_value(json).unwrap()
        };
        assert_eq!(webhook_token_of(&redacted, "hook"), "");

        let response = router.oneshot(put_config_request(&redacted)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(host.webhook_token("hook"), Some(TOKEN.to_string()));
        assert_eq!(host.webhook_token("forever"), Some(OTHER_TOKEN.to_string()));
    }
}
