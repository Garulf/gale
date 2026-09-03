#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Stopped,
    Pending,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiState {
    Reachable { warnings: usize },
    Unreachable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Running,
    Degraded,
    Stopped,
    Starting,
    NotInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayState {
    pub service: ServiceState,
    pub api: ApiState,
}

impl TrayState {
    pub fn health(self) -> Health {
        match self.service {
            ServiceState::Missing => Health::NotInstalled,
            ServiceState::Pending => Health::Starting,
            ServiceState::Stopped | ServiceState::Unknown => Health::Stopped,
            ServiceState::Running => match self.api {
                ApiState::Reachable { warnings: 0 } => Health::Running,
                ApiState::Reachable { warnings: _ } => Health::Degraded,
                ApiState::Unreachable => Health::Degraded,
            },
        }
    }

    pub fn status_label(self) -> String {
        match self.health() {
            Health::NotInstalled => "Gale: service not installed".to_string(),
            Health::Starting => "Gale: starting".to_string(),
            Health::Stopped => "Gale: stopped".to_string(),
            Health::Running => "Gale: running".to_string(),
            Health::Degraded => match self.api {
                ApiState::Reachable { warnings } => {
                    format!(
                        "Gale: running, {} warning{}",
                        warnings,
                        if warnings == 1 { "" } else { "s" }
                    )
                }
                ApiState::Unreachable => "Gale: running, UI unreachable".to_string(),
            },
        }
    }

    pub fn can_start(self) -> bool {
        matches!(self.service, ServiceState::Stopped)
    }

    pub fn can_stop(self) -> bool {
        matches!(self.service, ServiceState::Running | ServiceState::Pending)
    }
}

pub const UI_URL: &str = "http://127.0.0.1:5250";
pub const WARNINGS_URL: &str = "http://127.0.0.1:5250/api/warnings";

#[derive(serde::Deserialize)]
struct WarningsResponse {
    warnings: Vec<serde_json::Value>,
}

pub fn parse_warning_count(body: &str) -> Option<usize> {
    serde_json::from_str::<WarningsResponse>(body)
        .ok()
        .map(|response| response.warnings.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_service_states() -> [ServiceState; 5] {
        [
            ServiceState::Running,
            ServiceState::Stopped,
            ServiceState::Pending,
            ServiceState::Missing,
            ServiceState::Unknown,
        ]
    }

    fn all_api_states() -> [ApiState; 2] {
        [ApiState::Reachable { warnings: 0 }, ApiState::Unreachable]
    }

    #[test]
    fn health_and_label_table() {
        let cases: Vec<(ServiceState, ApiState, Health, &str)> = vec![
            (
                ServiceState::Missing,
                ApiState::Reachable { warnings: 0 },
                Health::NotInstalled,
                "Gale: service not installed",
            ),
            (
                ServiceState::Missing,
                ApiState::Unreachable,
                Health::NotInstalled,
                "Gale: service not installed",
            ),
            (
                ServiceState::Pending,
                ApiState::Reachable { warnings: 0 },
                Health::Starting,
                "Gale: starting",
            ),
            (
                ServiceState::Pending,
                ApiState::Unreachable,
                Health::Starting,
                "Gale: starting",
            ),
            (
                ServiceState::Stopped,
                ApiState::Reachable { warnings: 0 },
                Health::Stopped,
                "Gale: stopped",
            ),
            (
                ServiceState::Stopped,
                ApiState::Unreachable,
                Health::Stopped,
                "Gale: stopped",
            ),
            (
                ServiceState::Unknown,
                ApiState::Reachable { warnings: 0 },
                Health::Stopped,
                "Gale: stopped",
            ),
            (
                ServiceState::Unknown,
                ApiState::Unreachable,
                Health::Stopped,
                "Gale: stopped",
            ),
            (
                ServiceState::Running,
                ApiState::Reachable { warnings: 0 },
                Health::Running,
                "Gale: running",
            ),
            (
                ServiceState::Running,
                ApiState::Reachable { warnings: 2 },
                Health::Degraded,
                "Gale: running, 2 warnings",
            ),
            (
                ServiceState::Running,
                ApiState::Unreachable,
                Health::Degraded,
                "Gale: running, UI unreachable",
            ),
        ];

        for (service, api, expected_health, expected_label) in cases {
            let state = TrayState { service, api };
            assert_eq!(state.health(), expected_health, "{state:?}");
            assert_eq!(state.status_label(), expected_label, "{state:?}");
        }
    }

    #[test]
    fn every_combination_produces_a_health() {
        for service in all_service_states() {
            for api in all_api_states() {
                let state = TrayState { service, api };
                let _ = state.health();
                let _ = state.status_label();
            }
        }
    }

    #[test]
    fn single_warning_is_singular() {
        let state = TrayState {
            service: ServiceState::Running,
            api: ApiState::Reachable { warnings: 1 },
        };
        assert_eq!(state.status_label(), "Gale: running, 1 warning");
    }

    #[test]
    fn two_warnings_are_plural() {
        let state = TrayState {
            service: ServiceState::Running,
            api: ApiState::Reachable { warnings: 2 },
        };
        assert_eq!(state.status_label(), "Gale: running, 2 warnings");
    }

    #[test]
    fn can_start_only_when_stopped() {
        for service in all_service_states() {
            let state = TrayState {
                service,
                api: ApiState::Unreachable,
            };
            assert_eq!(state.can_start(), matches!(service, ServiceState::Stopped));
        }
    }

    #[test]
    fn can_stop_when_running_or_pending() {
        for service in all_service_states() {
            let state = TrayState {
                service,
                api: ApiState::Unreachable,
            };
            let expected = matches!(service, ServiceState::Running | ServiceState::Pending);
            assert_eq!(state.can_stop(), expected);
        }
    }

    #[test]
    fn parse_warning_count_from_list() {
        assert_eq!(parse_warning_count(r#"{"warnings":["a","b"]}"#), Some(2));
    }

    #[test]
    fn parse_warning_count_from_empty_list() {
        assert_eq!(parse_warning_count(r#"{"warnings":[]}"#), Some(0));
    }

    #[test]
    fn parse_warning_count_from_garbage() {
        assert_eq!(parse_warning_count("not json"), None);
    }
}
