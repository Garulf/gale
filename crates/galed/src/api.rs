use crate::config_store::ConfigStore;
use crate::engine_host::EngineHost;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use gale_core::config::{ConfigError, GaleConfig};
use gale_hw::Inventory;
use serde::Deserialize;
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
        .route("/profiles/:name/activate", post(activate_profile))
        .route("/controls/*id", put(set_control).delete(clear_control))
        .route("/ws", get(ws_upgrade))
        .layer(middleware::from_fn_with_state(ctx.clone(), require_api_key))
        .with_state(ctx.clone());
    Router::new().nest("/api", api).route("/", get(|| async { "gale" }))
}

async fn require_api_key(
    State(ctx): State<ApiContext>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if let Some(expected) = &ctx.api_key {
        let provided =
            request.headers().get("X-Api-Key").and_then(|v| v.to_str().ok());
        if provided != Some(expected.as_str()) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    next.run(request).await
}

async fn status(State(ctx): State<ApiContext>) -> Response {
    Json(ctx.host.subscribe().borrow().clone()).into_response()
}

async fn inventory(State(ctx): State<ApiContext>) -> Response {
    Json(ctx.inventory.read().unwrap().clone()).into_response()
}

async fn get_config(State(ctx): State<ApiContext>) -> Response {
    Json(ctx.host.config()).into_response()
}

fn config_error_response(error: ConfigError) -> Response {
    let status = match error {
        ConfigError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, error.to_string()).into_response()
}

async fn apply_and_persist(ctx: &ApiContext, config: GaleConfig) -> Result<(), Response> {
    ctx.host.replace_config(config.clone()).await.map_err(config_error_response)?;
    if let Err(error) = ctx.store.save(&config) {
        tracing::warn!(%error, "config persisted to disk failed");
    }
    Ok(())
}

async fn put_config(State(ctx): State<ApiContext>, Json(config): Json<GaleConfig>) -> Response {
    match apply_and_persist(&ctx, config).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(response) => response,
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
        Err(response) => response,
    }
}

async fn set_control(
    State(ctx): State<ApiContext>,
    Path(id): Path<String>,
    Json(body): Json<DutyBody>,
) -> Response {
    if !body.duty.is_finite() || !(0.0..=100.0).contains(&body.duty) {
        return (StatusCode::BAD_REQUEST, "duty must be a finite percent in 0..=100")
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
    use crate::config_store::ConfigStore;
    use crate::engine_host::EngineHost;
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use gale_core::config::GaleConfig;
    use gale_hw::{Backend, HwError, Id, Inventory};
    use http_body_util::BodyExt;
    use std::collections::HashMap;
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

    fn make_router(api_key: Option<String>) -> (axum::Router, Arc<EngineHost>) {
        let handle = BackendHandle::spawn(Box::new(NullBackend));
        let host =
            EngineHost::new(GaleConfig::from_toml(CONFIG).unwrap(), handle).unwrap();
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
}
