use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../ui/dist/"]
struct UiAssets;

pub async fn serve(request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    if let Some(file) = UiAssets::get(path) {
        return asset_response(path, file.data.into_owned());
    }
    match UiAssets::get("index.html") {
        Some(file) => asset_response("index.html", file.data.into_owned()),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn asset_response(path: &str, data: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime.essence_str().to_string())],
        Body::from(data),
    )
        .into_response()
}
