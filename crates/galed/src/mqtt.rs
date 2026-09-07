use std::collections::HashMap;
use std::time::Duration;

use gale_core::config::{GaleConfig, MqttConfig};
use gale_hw::{Id, Inventory, SensorKind};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Packet, QoS};
use serde_json::{json, Value};

use crate::api::ApiContext;
use crate::engine_host::Snapshot;

pub const FULL_REPUBLISH: Duration = Duration::from_secs(60);
pub const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub fn slug(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub struct Topics {
    prefix: String,
}

impl Topics {
    pub fn new(cfg: &MqttConfig) -> Self {
        Self {
            prefix: cfg.topic_prefix.trim_matches('/').to_string(),
        }
    }

    pub fn availability(&self) -> String {
        format!("{}/status", self.prefix)
    }

    pub fn sensor_state(&self, id: &str) -> String {
        format!("{}/sensor/{}/state", self.prefix, slug(id))
    }

    pub fn control_duty(&self, id: &str) -> String {
        format!("{}/control/{}/duty", self.prefix, slug(id))
    }

    pub fn control_manual(&self, id: &str) -> String {
        format!("{}/control/{}/manual", self.prefix, slug(id))
    }

    pub fn control_set_duty(&self, id: &str) -> String {
        format!("{}/control/{}/set_duty", self.prefix, slug(id))
    }

    pub fn control_set_manual(&self, id: &str) -> String {
        format!("{}/control/{}/set_manual", self.prefix, slug(id))
    }

    pub fn profile_state(&self) -> String {
        format!("{}/profile/state", self.prefix)
    }

    pub fn profile_set(&self) -> String {
        format!("{}/profile/set", self.prefix)
    }

    pub fn subscriptions(&self) -> Vec<String> {
        vec![
            format!("{}/control/+/set_duty", self.prefix),
            format!("{}/control/+/set_manual", self.prefix),
            self.profile_set(),
        ]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VirtualInfo {
    pub id: Id,
    pub kind: &'static str,
}

pub struct Device {
    pub hostname: String,
}

impl Device {
    fn json(&self) -> Value {
        json!({
            "identifiers": [format!("gale_{}", slug(&self.hostname))],
            "name": format!("Gale ({})", self.hostname),
            "manufacturer": "Gale",
            "model": "fan control daemon",
            "sw_version": env!("CARGO_PKG_VERSION"),
        })
    }
}

fn sensor_unit(kind: SensorKind) -> (&'static str, Option<&'static str>) {
    match kind {
        SensorKind::Temp => ("°C", Some("temperature")),
        SensorKind::Rpm => ("RPM", None),
        SensorKind::Duty | SensorKind::Percent => ("%", None),
        SensorKind::Clock => ("MHz", None),
        SensorKind::Memory => ("MiB", Some("data_size")),
        SensorKind::Power => ("W", Some("power")),
        SensorKind::State => ("", None),
    }
}

fn base(
    topics: &Topics,
    device: &Device,
    name: &str,
    unique: &str,
) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    map.insert("name".into(), json!(name));
    map.insert(
        "unique_id".into(),
        json!(format!("gale_{}_{unique}", slug(&device.hostname))),
    );
    map.insert("availability_topic".into(), json!(topics.availability()));
    map.insert("device".into(), device.json());
    map
}

pub fn discovery(
    cfg: &MqttConfig,
    inventory: &Inventory,
    virtual_sensors: &[VirtualInfo],
    labels: &std::collections::BTreeMap<Id, String>,
    profiles: &[String],
    device: &Device,
) -> Vec<(String, Value)> {
    let topics = Topics::new(cfg);
    let prefix = cfg.discovery_prefix.trim_matches('/');
    let label_of = |id: &str, fallback: &str| {
        labels
            .get(id)
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    };
    let mut out = Vec::new();
    for sensor in &inventory.sensors {
        let (unit, class) = sensor_unit(sensor.kind);
        let mut payload = base(
            &topics,
            device,
            &label_of(&sensor.id, &sensor.label),
            &slug(&sensor.id),
        );
        payload.insert("state_topic".into(), json!(topics.sensor_state(&sensor.id)));
        if !unit.is_empty() {
            payload.insert("unit_of_measurement".into(), json!(unit));
        }
        payload.insert("state_class".into(), json!("measurement"));
        if let Some(class) = class {
            payload.insert("device_class".into(), json!(class));
        }
        out.push((
            format!("{prefix}/sensor/gale_{}/config", slug(&sensor.id)),
            Value::Object(payload),
        ));
    }
    for virtual_sensor in virtual_sensors {
        let name = virtual_sensor.id.trim_start_matches("virtual/");
        let mut payload = base(&topics, device, name, &slug(&virtual_sensor.id));
        payload.insert(
            "state_topic".into(),
            json!(topics.sensor_state(&virtual_sensor.id)),
        );
        payload.insert("state_class".into(), json!("measurement"));
        if virtual_sensor.kind == "delta" {
            payload.insert("unit_of_measurement".into(), json!("°C/min"));
        } else {
            payload.insert("unit_of_measurement".into(), json!("°C"));
            payload.insert("device_class".into(), json!("temperature"));
        }
        out.push((
            format!("{prefix}/sensor/gale_{}/config", slug(&virtual_sensor.id)),
            Value::Object(payload),
        ));
    }
    for control in &inventory.controls {
        let label = label_of(&control.id, &control.label);
        let mut duty = base(
            &topics,
            device,
            &format!("{label} duty"),
            &format!("{}_duty", slug(&control.id)),
        );
        duty.insert(
            "state_topic".into(),
            json!(topics.control_duty(&control.id)),
        );
        duty.insert(
            "command_topic".into(),
            json!(topics.control_set_duty(&control.id)),
        );
        duty.insert("min".into(), json!(0));
        duty.insert("max".into(), json!(100));
        duty.insert("step".into(), json!(1));
        duty.insert("mode".into(), json!("slider"));
        duty.insert("unit_of_measurement".into(), json!("%"));
        duty.insert("icon".into(), json!("mdi:fan"));
        out.push((
            format!("{prefix}/number/gale_{}_duty/config", slug(&control.id)),
            Value::Object(duty),
        ));
        let mut manual = base(
            &topics,
            device,
            &format!("{label} manual"),
            &format!("{}_manual", slug(&control.id)),
        );
        manual.insert(
            "state_topic".into(),
            json!(topics.control_manual(&control.id)),
        );
        manual.insert(
            "command_topic".into(),
            json!(topics.control_set_manual(&control.id)),
        );
        manual.insert("payload_on".into(), json!("ON"));
        manual.insert("payload_off".into(), json!("OFF"));
        manual.insert("icon".into(), json!("mdi:hand-back-right"));
        out.push((
            format!("{prefix}/switch/gale_{}_manual/config", slug(&control.id)),
            Value::Object(manual),
        ));
    }
    let mut profile = base(&topics, device, "Gale profile", "profile");
    profile.insert("state_topic".into(), json!(topics.profile_state()));
    profile.insert("command_topic".into(), json!(topics.profile_set()));
    profile.insert("options".into(), json!(profiles));
    profile.insert("icon".into(), json!("mdi:tune"));
    out.push((
        format!("{prefix}/select/gale_profile/config"),
        Value::Object(profile),
    ));
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetDuty(Id, f64),
    SetManual(Id, bool),
    ActivateProfile(String),
}

pub fn parse_command(cfg: &MqttConfig, ids: &[Id], topic: &str, payload: &str) -> Option<Command> {
    let topics = Topics::new(cfg);
    let payload = payload.trim();
    if topic == topics.profile_set() {
        return (!payload.is_empty()).then(|| Command::ActivateProfile(payload.to_string()));
    }
    for id in ids {
        if topic == topics.control_set_duty(id) {
            let duty: f64 = payload.parse().ok()?;
            return duty
                .is_finite()
                .then_some(Command::SetDuty(id.clone(), duty.clamp(0.0, 100.0)));
        }
        if topic == topics.control_set_manual(id) {
            return match payload.to_ascii_uppercase().as_str() {
                "ON" | "TRUE" | "1" => Some(Command::SetManual(id.clone(), true)),
                "OFF" | "FALSE" | "0" => Some(Command::SetManual(id.clone(), false)),
                _ => None,
            };
        }
    }
    None
}

fn format_value(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.2}", v)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string(),
        None => "unavailable".to_string(),
    }
}

#[derive(Default)]
pub struct StateTracker {
    published: HashMap<String, String>,
    last_full: Option<std::time::Instant>,
}

impl StateTracker {
    pub fn forget(&mut self, topic: &str) {
        self.published.remove(topic);
    }

    pub fn messages(
        &mut self,
        topics: &Topics,
        snapshot: &Snapshot,
        control_ids: &[Id],
        now: std::time::Instant,
    ) -> Vec<(String, String)> {
        let force = self
            .last_full
            .is_none_or(|t| now.duration_since(t) >= FULL_REPUBLISH);
        if force {
            self.last_full = Some(now);
        }
        let mut wanted: Vec<(String, String)> = snapshot
            .sensors
            .iter()
            .map(|(id, value)| (topics.sensor_state(id), format_value(*value)))
            .collect();
        for id in control_ids {
            wanted.push((
                topics.control_duty(id),
                format_value(snapshot.duties.get(id).copied()),
            ));
            wanted.push((
                topics.control_manual(id),
                if snapshot.overrides.contains(id) {
                    "ON"
                } else {
                    "OFF"
                }
                .to_string(),
            ));
        }
        wanted.push((topics.profile_state(), snapshot.active_profile.clone()));
        wanted.sort();
        let mut out = Vec::new();
        for (topic, payload) in wanted {
            if force || self.published.get(&topic) != Some(&payload) {
                self.published.insert(topic.clone(), payload.clone());
                out.push((topic, payload));
            }
        }
        out
    }
}

fn virtual_infos(config: &GaleConfig) -> Vec<VirtualInfo> {
    config
        .profiles
        .get(&config.active_profile)
        .map(|profile| {
            profile
                .sensors
                .iter()
                .map(|(name, cfg)| VirtualInfo {
                    id: gale_core::config::virtual_id(name),
                    kind: cfg.type_name(),
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn apply(ctx: &ApiContext, snapshot: &Snapshot, command: Command) {
    let result = match command {
        Command::SetDuty(id, duty) => ctx
            .host
            .set_manual(&id, duty)
            .await
            .map_err(|e| e.to_string()),
        Command::SetManual(id, true) => {
            let duty = snapshot.duties.get(&id).copied().unwrap_or(50.0);
            ctx.host
                .set_manual(&id, duty)
                .await
                .map_err(|e| e.to_string())
        }
        Command::SetManual(id, false) => {
            ctx.host.clear_manual(&id).await.map_err(|e| e.to_string())
        }
        Command::ActivateProfile(name) => crate::api::activate_profile_named(ctx, &name).await,
    };
    if let Err(error) = result {
        tracing::warn!(%error, "mqtt command failed");
    }
}

fn discovery_fingerprint(config: &GaleConfig) -> String {
    let virtuals: Vec<String> = virtual_infos(config).into_iter().map(|v| v.id).collect();
    format!(
        "{:?}|{:?}|{:?}",
        config.profiles.keys().collect::<Vec<_>>(),
        virtuals,
        config.labels
    )
}

async fn announce(
    client: &AsyncClient,
    ctx: &ApiContext,
    cfg: &MqttConfig,
    topics: &Topics,
    device: &Device,
) {
    let config = ctx.host.config();
    let inventory = ctx.inventory.read().unwrap().clone();
    let profiles: Vec<String> = config.profiles.keys().cloned().collect();
    for (topic, payload) in discovery(
        cfg,
        &inventory,
        &virtual_infos(&config),
        &config.labels,
        &profiles,
        device,
    ) {
        if let Err(error) = client
            .publish(topic, QoS::AtLeastOnce, true, payload.to_string())
            .await
        {
            tracing::warn!(%error, "mqtt discovery publish failed");
            return;
        }
    }
    for topic in topics.subscriptions() {
        let _ = client.subscribe(topic, QoS::AtLeastOnce).await;
    }
    let _ = client
        .publish(topics.availability(), QoS::AtLeastOnce, true, "online")
        .await;
}

pub async fn run(ctx: ApiContext, cfg: MqttConfig) {
    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string();
    let device = std::sync::Arc::new(Device {
        hostname: if hostname.is_empty() {
            "host".to_string()
        } else {
            hostname
        },
    });
    let cfg = std::sync::Arc::new(cfg);
    let topics = std::sync::Arc::new(Topics::new(&cfg));
    let mut options = MqttOptions::new(cfg.client_id.clone(), cfg.host.clone(), cfg.port);
    options.set_keep_alive(Duration::from_secs(30));
    if let (Some(user), Some(pass)) = (&cfg.username, &cfg.password) {
        options.set_credentials(user.clone(), pass.clone());
    }
    options.set_last_will(LastWill::new(
        topics.availability(),
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (client, mut eventloop) = AsyncClient::new(options, 256);
    let mut receiver = ctx.host.subscribe();
    let mut tracker = StateTracker::default();
    let mut control_ids: Vec<Id> = ctx
        .inventory
        .read()
        .unwrap()
        .controls
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let mut fingerprint = String::new();
    let mut ticks_since_check = 0u32;
    let spawn_announce = |client: &AsyncClient| {
        let client = client.clone();
        let ctx = ctx.clone();
        let cfg = cfg.clone();
        let topics = topics.clone();
        let device = device.clone();
        tokio::spawn(async move { announce(&client, &ctx, &cfg, &topics, &device).await });
    };
    loop {
        tokio::select! {
            event = eventloop.poll() => match event {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    tracing::info!(host = %cfg.host, port = cfg.port, "mqtt connected");
                    control_ids = ctx.inventory.read().unwrap().controls.iter().map(|c| c.id.clone()).collect();
                    fingerprint = discovery_fingerprint(&ctx.host.config());
                    tracker = StateTracker::default();
                    spawn_announce(&client);
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    let payload = String::from_utf8_lossy(&publish.payload).to_string();
                    if let Some(command) = parse_command(&cfg, &control_ids, &publish.topic, &payload) {
                        tracing::info!(topic = %publish.topic, ?command, "mqtt command");
                        let snapshot = receiver.borrow().clone();
                        let ctx = ctx.clone();
                        tokio::spawn(async move { apply(&ctx, &snapshot, command).await });
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, "mqtt connection error, retrying");
                    tokio::time::sleep(RECONNECT_DELAY).await;
                }
            },
            changed = receiver.changed() => {
                if changed.is_err() {
                    return;
                }
                ticks_since_check += 1;
                if ticks_since_check >= 5 {
                    ticks_since_check = 0;
                    let current = discovery_fingerprint(&ctx.host.config());
                    if current != fingerprint {
                        fingerprint = current;
                        tracing::info!("mqtt discovery refreshed after a config change");
                        spawn_announce(&client);
                    }
                }
                let snapshot = receiver.borrow_and_update().clone();
                for (topic, payload) in tracker.messages(&topics, &snapshot, &control_ids, std::time::Instant::now()) {
                    if client.try_publish(topic.clone(), QoS::AtMostOnce, true, payload).is_err() {
                        tracker.forget(&topic);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_hw::{ControlInfo, SensorInfo};
    use std::collections::{BTreeMap, HashSet};

    fn cfg() -> MqttConfig {
        MqttConfig::default()
    }

    fn inventory() -> Inventory {
        Inventory {
            sensors: vec![
                SensorInfo {
                    id: "hwmon/x/temp1".into(),
                    label: "CPUTIN".into(),
                    kind: SensorKind::Temp,
                },
                SensorInfo {
                    id: "hwmon/x/fan1".into(),
                    label: "fan1".into(),
                    kind: SensorKind::Rpm,
                },
            ],
            controls: vec![ControlInfo {
                id: "hwmon/x/pwm1".into(),
                label: "pwm1".into(),
            }],
        }
    }

    #[test]
    fn slug_flattens_ids_into_topic_safe_segments() {
        assert_eq!(
            slug("corsair/commander-pro-0805/fan1"),
            "corsair_commander_pro_0805_fan1"
        );
        assert_eq!(slug("virtual/CPU Hot"), "virtual_cpu_hot");
    }

    #[test]
    fn discovery_describes_sensors_controls_and_the_profile_select() {
        let mut labels = BTreeMap::new();
        labels.insert("hwmon/x/pwm1".to_string(), "Front intake".to_string());
        let virtuals = vec![
            VirtualInfo {
                id: "virtual/hot".into(),
                kind: "max",
            },
            VirtualInfo {
                id: "virtual/rate".into(),
                kind: "delta",
            },
        ];
        let out = discovery(
            &cfg(),
            &inventory(),
            &virtuals,
            &labels,
            &["default".into(), "quiet".into()],
            &Device {
                hostname: "albedo".into(),
            },
        );
        let topics: Vec<&str> = out.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            topics,
            vec![
                "homeassistant/sensor/gale_hwmon_x_temp1/config",
                "homeassistant/sensor/gale_hwmon_x_fan1/config",
                "homeassistant/sensor/gale_virtual_hot/config",
                "homeassistant/sensor/gale_virtual_rate/config",
                "homeassistant/number/gale_hwmon_x_pwm1_duty/config",
                "homeassistant/switch/gale_hwmon_x_pwm1_manual/config",
                "homeassistant/select/gale_profile/config",
            ]
        );
        let temp = &out[0].1;
        assert_eq!(temp["device_class"], "temperature");
        assert_eq!(temp["unit_of_measurement"], "°C");
        assert_eq!(temp["state_topic"], "gale/sensor/hwmon_x_temp1/state");
        assert_eq!(temp["availability_topic"], "gale/status");
        assert_eq!(temp["device"]["identifiers"][0], "gale_albedo");
        assert_eq!(temp["unique_id"], "gale_albedo_hwmon_x_temp1");
        assert_eq!(out[1].1["unit_of_measurement"], "RPM");
        assert!(out[1].1.get("device_class").is_none());
        assert_eq!(out[3].1["unit_of_measurement"], "°C/min");
        let duty = &out[4].1;
        assert_eq!(duty["name"], "Front intake duty");
        assert_eq!(duty["command_topic"], "gale/control/hwmon_x_pwm1/set_duty");
        assert_eq!(duty["max"], 100);
        let manual = &out[5].1;
        assert_eq!(
            manual["command_topic"],
            "gale/control/hwmon_x_pwm1/set_manual"
        );
        assert_eq!(out[6].1["options"], json!(["default", "quiet"]));
    }

    #[test]
    fn commands_are_parsed_from_their_topics_and_clamped() {
        let ids = vec!["hwmon/x/pwm1".to_string()];
        assert_eq!(
            parse_command(&cfg(), &ids, "gale/control/hwmon_x_pwm1/set_duty", "42.5"),
            Some(Command::SetDuty("hwmon/x/pwm1".into(), 42.5))
        );
        assert_eq!(
            parse_command(&cfg(), &ids, "gale/control/hwmon_x_pwm1/set_duty", "250"),
            Some(Command::SetDuty("hwmon/x/pwm1".into(), 100.0))
        );
        assert_eq!(
            parse_command(&cfg(), &ids, "gale/control/hwmon_x_pwm1/set_duty", "hot"),
            None
        );
        assert_eq!(
            parse_command(&cfg(), &ids, "gale/control/hwmon_x_pwm1/set_manual", "OFF"),
            Some(Command::SetManual("hwmon/x/pwm1".into(), false))
        );
        assert_eq!(
            parse_command(&cfg(), &ids, "gale/profile/set", "quiet"),
            Some(Command::ActivateProfile("quiet".into()))
        );
        assert_eq!(
            parse_command(&cfg(), &ids, "gale/control/hwmon_x_pwm9/set_duty", "10"),
            None
        );
    }

    #[test]
    fn state_tracker_publishes_changes_only_until_the_periodic_full_refresh() {
        let topics = Topics::new(&cfg());
        let mut snapshot = Snapshot {
            sensors: HashMap::from([
                ("hwmon/x/temp1".to_string(), Some(45.5)),
                ("hwmon/x/fan1".to_string(), None),
            ]),
            curves: HashMap::new(),
            duties: HashMap::from([("hwmon/x/pwm1".to_string(), 33.0)]),
            manual: HashMap::new(),
            overrides: HashSet::new(),
            active_profile: "default".into(),
        };
        let ids = vec!["hwmon/x/pwm1".to_string()];
        let mut tracker = StateTracker::default();
        let start = std::time::Instant::now();
        let first = tracker.messages(&topics, &snapshot, &ids, start);
        assert_eq!(
            first,
            vec![
                (
                    "gale/control/hwmon_x_pwm1/duty".to_string(),
                    "33".to_string()
                ),
                (
                    "gale/control/hwmon_x_pwm1/manual".to_string(),
                    "OFF".to_string()
                ),
                ("gale/profile/state".to_string(), "default".to_string()),
                (
                    "gale/sensor/hwmon_x_fan1/state".to_string(),
                    "unavailable".to_string()
                ),
                (
                    "gale/sensor/hwmon_x_temp1/state".to_string(),
                    "45.5".to_string()
                ),
            ]
        );
        assert!(tracker
            .messages(&topics, &snapshot, &ids, start + Duration::from_secs(1))
            .is_empty());
        snapshot.overrides.insert("hwmon/x/pwm1".into());
        let changed = tracker.messages(&topics, &snapshot, &ids, start + Duration::from_secs(2));
        assert_eq!(
            changed,
            vec![(
                "gale/control/hwmon_x_pwm1/manual".to_string(),
                "ON".to_string()
            )]
        );
        tracker.forget("gale/control/hwmon_x_pwm1/manual");
        assert_eq!(
            tracker
                .messages(&topics, &snapshot, &ids, start + Duration::from_secs(3))
                .len(),
            1
        );
        assert_eq!(
            tracker
                .messages(&topics, &snapshot, &ids, start + FULL_REPUBLISH)
                .len(),
            5
        );
    }
}
