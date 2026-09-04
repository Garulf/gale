use gale_core::config::{virtual_id, GaleConfig, ProfileConfig, VirtualSensorConfig};
use gale_hw::Id;
use rand::RngCore;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
struct WebhookEntry {
    token: String,
    timeout_s: Option<f64>,
    value: Option<f64>,
    received_at: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct WebhookStore {
    entries: HashMap<String, WebhookEntry>,
}

fn webhook_sensors(config: &GaleConfig) -> impl Iterator<Item = (&String, &String, Option<f64>)> {
    config
        .profiles
        .get(&config.active_profile)
        .map(|profile: &ProfileConfig| &profile.sensors)
        .into_iter()
        .flatten()
        .filter_map(|(name, cfg)| match cfg {
            VirtualSensorConfig::Webhook { token, timeout_s } => Some((name, token, *timeout_s)),
            _ => None,
        })
}

impl WebhookStore {
    pub fn from_config(config: &GaleConfig) -> Self {
        let mut store = Self::default();
        store.reconcile(config);
        store
    }

    pub fn reconcile(&mut self, config: &GaleConfig) {
        let mut next = HashMap::new();
        for (name, token, timeout_s) in webhook_sensors(config) {
            let carried = self
                .entries
                .get(name)
                .filter(|entry| entry.token == *token)
                .map(|entry| (entry.value, entry.received_at))
                .unwrap_or((None, None));
            next.insert(
                name.clone(),
                WebhookEntry {
                    token: token.clone(),
                    timeout_s,
                    value: carried.0,
                    received_at: carried.1,
                },
            );
        }
        self.entries = next;
    }

    pub fn record(&mut self, token: &str, value: f64, now: Instant) -> bool {
        match self.entries.values_mut().find(|entry| entry.token == token) {
            Some(entry) => {
                entry.value = Some(value);
                entry.received_at = Some(now);
                true
            }
            None => false,
        }
    }

    pub fn seed(&self, sensors: &mut HashMap<Id, Option<f64>>, now: Instant) {
        for (name, entry) in &self.entries {
            let fresh = match (entry.received_at, entry.timeout_s) {
                (None, _) => false,
                (Some(_), None) => true,
                (Some(at), Some(limit)) => now.duration_since(at).as_secs_f64() <= limit,
            };
            sensors.insert(virtual_id(name), if fresh { entry.value } else { None });
        }
    }

    pub fn token_for(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(|entry| entry.token.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn fill_missing_tokens(config: &mut GaleConfig, existing: &GaleConfig) {
    for (profile_name, profile) in &mut config.profiles {
        for (name, cfg) in &mut profile.sensors {
            if let VirtualSensorConfig::Webhook { token, .. } = cfg {
                if !token.is_empty() {
                    continue;
                }
                let reused = existing
                    .profiles
                    .get(profile_name)
                    .and_then(|old| old.sensors.get(name))
                    .and_then(|old| match old {
                        VirtualSensorConfig::Webhook { token, .. } if !token.is_empty() => {
                            Some(token.clone())
                        }
                        _ => None,
                    });
                *token = reused.unwrap_or_else(generate_token);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER_TOKEN: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    const WEBHOOK_CONFIG: &str = r#"
active_profile = "p"

[profiles.p.sensors.hook]
type = "webhook"
token = "{TOKEN}"
timeout_s = 60.0

[profiles.p.sensors.forever]
type = "webhook"
token = "{OTHER_TOKEN}"

[profiles.p.sensors.cpu_hot]
type = "max"
inputs = ["t1", "t2"]

[profiles.p.curves.c]
type = "flat"
duty = 50.0

[profiles.p.assignments]
"pwm1" = "c"
"#;

    fn config(toml: &str) -> GaleConfig {
        GaleConfig::from_toml(
            &toml
                .replace("{TOKEN}", TOKEN)
                .replace("{OTHER_TOKEN}", OTHER_TOKEN),
        )
        .unwrap()
    }

    fn at(t0: Instant, secs: f64) -> Instant {
        t0 + Duration::from_secs_f64(secs)
    }

    fn seeded(store: &WebhookStore, now: Instant) -> HashMap<String, Option<f64>> {
        let mut map = HashMap::new();
        store.seed(&mut map, now);
        map
    }

    #[test]
    fn from_config_registers_webhooks_of_the_active_profile_only() {
        let store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        assert_eq!(store.len(), 2);
        assert_eq!(store.token_for("hook"), Some(TOKEN));
        assert_eq!(store.token_for("forever"), Some(OTHER_TOKEN));
        assert_eq!(store.token_for("cpu_hot"), None);

        let inactive = WEBHOOK_CONFIG.replace(
            "active_profile = \"p\"",
            "active_profile = \"q\"\n[profiles.q.curves.c]\ntype = \"flat\"\nduty = 50.0\n",
        );
        let store = WebhookStore::from_config(&config(&inactive));
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn seed_before_any_post_yields_none() {
        let t0 = Instant::now();
        let store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        let map = seeded(&store, t0);
        assert_eq!(map["virtual/hook"], None);
        assert_eq!(map["virtual/forever"], None);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn record_unknown_token_returns_false_and_records_nothing() {
        let t0 = Instant::now();
        let mut store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        assert!(!store.record(&"f".repeat(64), 1.0, t0));
        let map = seeded(&store, t0);
        assert_eq!(map["virtual/hook"], None);
        assert_eq!(map["virtual/forever"], None);
    }

    #[test]
    fn record_then_seed_yields_the_value() {
        let t0 = Instant::now();
        let mut store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        assert!(store.record(TOKEN, 51.5, t0));
        let map = seeded(&store, at(t0, 1.0));
        assert_eq!(map["virtual/hook"], Some(51.5));
        assert_eq!(map["virtual/forever"], None);
    }

    #[test]
    fn seed_is_none_once_the_value_is_older_than_timeout() {
        let t0 = Instant::now();
        let mut store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        store.record(TOKEN, 51.5, t0);
        assert_eq!(seeded(&store, at(t0, 59.9))["virtual/hook"], Some(51.5));
        assert_eq!(seeded(&store, at(t0, 60.1))["virtual/hook"], None);
    }

    #[test]
    fn seed_never_expires_without_timeout() {
        let t0 = Instant::now();
        let mut store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        store.record(OTHER_TOKEN, 7.0, t0);
        let map = seeded(&store, at(t0, 86400.0 * 365.0));
        assert_eq!(map["virtual/forever"], Some(7.0));
    }

    #[test]
    fn reconcile_keeps_value_for_same_name_and_token_and_applies_new_timeout() {
        let t0 = Instant::now();
        let mut store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        store.record(TOKEN, 51.5, t0);
        let shorter = WEBHOOK_CONFIG.replace("timeout_s = 60.0", "timeout_s = 1.0");
        store.reconcile(&config(&shorter));
        assert_eq!(seeded(&store, at(t0, 0.5))["virtual/hook"], Some(51.5));
        assert_eq!(seeded(&store, at(t0, 30.0))["virtual/hook"], None);
    }

    #[test]
    fn reconcile_resets_renamed_drops_removed_and_adds_new() {
        let t0 = Instant::now();
        let mut store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        store.record(TOKEN, 51.5, t0);
        store.record(OTHER_TOKEN, 7.0, t0);
        let third = "a".repeat(64);
        let renamed = format!(
            r#"
active_profile = "p"

[profiles.p.sensors.hook2]
type = "webhook"
token = "{TOKEN}"
timeout_s = 60.0

[profiles.p.sensors.third]
type = "webhook"
token = "{third}"

[profiles.p.curves.c]
type = "flat"
duty = 50.0

[profiles.p.assignments]
"pwm1" = "c"
"#
        );
        store.reconcile(&config(&renamed));
        assert_eq!(store.len(), 2);
        assert_eq!(store.token_for("hook"), None);
        let map = seeded(&store, at(t0, 1.0));
        assert_eq!(map["virtual/hook2"], None);
        assert_eq!(map["virtual/third"], None);
        assert!(!map.contains_key("virtual/forever"));
    }

    #[test]
    fn reconcile_resets_when_the_token_changes_under_the_same_name() {
        let t0 = Instant::now();
        let mut store = WebhookStore::from_config(&config(WEBHOOK_CONFIG));
        store.record(TOKEN, 51.5, t0);
        let replacement = "b".repeat(64);
        let changed = WEBHOOK_CONFIG.replacen("{TOKEN}", &replacement, 1);
        store.reconcile(&config(&changed));
        assert_eq!(seeded(&store, t0)["virtual/hook"], None);
        assert!(store.record(&replacement, 1.0, t0));
        assert!(!store.record(TOKEN, 1.0, t0));
    }

    #[test]
    fn generate_token_is_64_lowercase_hex_and_unique() {
        let token = generate_token();
        assert_eq!(token.len(), 64);
        assert!(token
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn fill_missing_tokens_generates_reuses_and_preserves() {
        let mut incoming = config(
            r#"
active_profile = "p"

[profiles.p.sensors.fresh]
type = "webhook"
token = ""

[profiles.p.sensors.hook]
type = "webhook"
token = ""

[profiles.p.curves.c]
type = "flat"
duty = 50.0

[profiles.p.assignments]
"pwm1" = "c"

[profiles.q.sensors.other]
type = "webhook"
token = "keep"

[profiles.q.curves.c]
type = "flat"
duty = 50.0
"#,
        );
        let existing = config(WEBHOOK_CONFIG);
        fill_missing_tokens(&mut incoming, &existing);

        let token_of = |profile: &str, name: &str| match &incoming.profiles[profile].sensors[name] {
            VirtualSensorConfig::Webhook { token, .. } => token.clone(),
            other => panic!("expected webhook, got {other:?}"),
        };
        let fresh = token_of("p", "fresh");
        assert_eq!(fresh.len(), 64);
        assert!(fresh.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(token_of("p", "hook"), TOKEN);
        assert_eq!(token_of("q", "other"), "keep");
    }
}
