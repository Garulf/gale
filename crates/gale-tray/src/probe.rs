use std::time::Duration;

use crate::service;
use crate::state::{parse_warning_count, ApiState, TrayState, WARNINGS_URL};

const TIMEOUT: Duration = Duration::from_millis(500);

pub fn api_state() -> ApiState {
    let request = ureq::get(WARNINGS_URL)
        .config()
        .timeout_global(Some(TIMEOUT))
        .proxy(None)
        .build();

    let mut response = match request.call() {
        Ok(response) if response.status().as_u16() == 200 => response,
        _ => return ApiState::Unreachable,
    };

    match response.body_mut().read_to_string() {
        Ok(body) => ApiState::Reachable {
            warnings: parse_warning_count(&body).unwrap_or(0),
        },
        Err(_) => ApiState::Unreachable,
    }
}

pub fn current_state() -> TrayState {
    TrayState {
        service: service::query(),
        api: api_state(),
    }
}
