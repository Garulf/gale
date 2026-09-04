use clap::{Parser, Subcommand};
use gale_core::config::GaleConfig;
use serde_json::Value;

#[derive(Parser)]
#[command(name = "gale", about = "Gale fan control CLI")]
struct Cli {
    #[arg(
        long,
        global = true,
        env = "GALE_URL",
        default_value = "http://127.0.0.1:5250"
    )]
    url: String,
    #[arg(long, global = true, env = "GALE_API_KEY")]
    api_key: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Status,
    Inventory,
    Set { id: String, pct: f64 },
    Release { id: String },
    Profile { name: String },
    Config,
}

fn main() {
    let cli = Cli::parse();
    let result = run(&cli);
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<(), String> {
    match &cli.command {
        Command::Status => {
            let status: Value = get_json(cli, "/api/status")?;
            print!("{}", render_status(&status));
            Ok(())
        }
        Command::Inventory => {
            let inventory: Value = get_json(cli, "/api/inventory")?;
            print!("{}", render_inventory(&inventory));
            Ok(())
        }
        Command::Set { id, pct } => send(
            cli,
            "PUT",
            &format!("/api/controls/{}", encode_id_path(id)),
            Some(serde_json::json!({ "duty": pct })),
        ),
        Command::Release { id } => send(
            cli,
            "DELETE",
            &format!("/api/controls/{}", encode_id_path(id)),
            None,
        ),
        Command::Profile { name } => send(
            cli,
            "POST",
            &format!("/api/profiles/{}/activate", encode_segment(name)),
            None,
        ),
        Command::Config => {
            let config: GaleConfig =
                serde_json::from_value(get_json(cli, "/api/config")?).map_err(|e| e.to_string())?;
            print!("{}", config.to_toml().map_err(|e| e.to_string())?);
            Ok(())
        }
    }
}

fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn encode_id_path(id: &str) -> String {
    id.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn request(cli: &Cli, method: &str, path: &str) -> ureq::Request {
    let mut req = ureq::request(method, &format!("{}{path}", cli.url));
    if let Some(key) = &cli.api_key {
        req = req.set("X-Api-Key", key);
    }
    req
}

fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            format!("{code}: {}", response.into_string().unwrap_or_default())
        }
        other => other.to_string(),
    }
}

fn get_json(cli: &Cli, path: &str) -> Result<Value, String> {
    request(cli, "GET", path)
        .call()
        .map_err(describe)?
        .into_json()
        .map_err(|e| e.to_string())
}

fn send(cli: &Cli, method: &str, path: &str, body: Option<Value>) -> Result<(), String> {
    let req = request(cli, method, path);
    let result = match body {
        Some(json) => req.send_json(json),
        None => req.call(),
    };
    result.map(|_| ()).map_err(describe)
}

fn render_status(status: &Value) -> String {
    let mut out = String::from("sensors:\n");
    push_map(&mut out, &status["sensors"], |v| {
        v.as_f64()
            .map(|n| format!("{n:.1}"))
            .unwrap_or_else(|| "n/a".into())
    });
    out.push_str("controls:\n");
    let manual = &status["manual"];
    if let Some(duties) = status["duties"].as_object() {
        let mut keys: Vec<_> = duties.keys().collect();
        keys.sort();
        for key in keys {
            let duty = duties[key].as_f64().unwrap_or(0.0);
            let marker = if manual.get(key).is_some() {
                " (manual)"
            } else {
                ""
            };
            out.push_str(&format!("  {key}  {duty:.1}%{marker}\n"));
        }
    }
    out
}

fn render_inventory(inventory: &Value) -> String {
    let mut out = String::from("sensors:\n");
    for sensor in inventory["sensors"].as_array().unwrap_or(&Vec::new()) {
        out.push_str(&format!(
            "  {}  [{}]  {}\n",
            sensor["id"].as_str().unwrap_or(""),
            sensor["kind"].as_str().unwrap_or(""),
            sensor["label"].as_str().unwrap_or("")
        ));
    }
    out.push_str("controls:\n");
    for control in inventory["controls"].as_array().unwrap_or(&Vec::new()) {
        out.push_str(&format!(
            "  {}  {}\n",
            control["id"].as_str().unwrap_or(""),
            control["label"].as_str().unwrap_or("")
        ));
    }
    if let Some(virtual_sensors) = inventory["virtual"].as_array() {
        if !virtual_sensors.is_empty() {
            out.push_str("virtual:\n");
            for sensor in virtual_sensors {
                let inputs: Vec<&str> = sensor["inputs"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .map(|v| v.as_str().unwrap_or(""))
                    .collect();
                out.push_str(&format!(
                    "  {}  [{}]  {}\n",
                    sensor["id"].as_str().unwrap_or(""),
                    sensor["type"].as_str().unwrap_or(""),
                    inputs.join(", ")
                ));
            }
        }
    }
    out
}

fn push_map(out: &mut String, map: &Value, fmt: impl Fn(&Value) -> String) {
    if let Some(object) = map.as_object() {
        let mut keys: Vec<_> = object.keys().collect();
        keys.sort();
        for key in keys {
            out.push_str(&format!("  {key}  {}\n", fmt(&object[key])));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_status_sorts_and_marks_manual() {
        let status = serde_json::json!({
            "sensors": {"b/t": 45.5, "a/t": null},
            "duties": {"p2": 70.0, "p1": 42.0},
            "manual": {"p2": 70.0},
            "active_profile": "default"
        });
        let text = render_status(&status);
        assert_eq!(
            text,
            "sensors:\n  a/t  n/a\n  b/t  45.5\ncontrols:\n  p1  42.0%\n  p2  70.0% (manual)\n"
        );
    }

    #[test]
    fn encode_id_path_percent_encodes_each_segment_and_keeps_slashes() {
        assert_eq!(
            encode_id_path("hwmon/nct6798/pwm 1"),
            "hwmon/nct6798/pwm%201"
        );
        assert_eq!(encode_id_path("a/b#c"), "a/b%23c");
    }

    #[test]
    fn encode_segment_percent_encodes_reserved_characters() {
        assert_eq!(encode_segment("quiet profile"), "quiet%20profile");
        assert_eq!(encode_segment("plain-name_1.2~3"), "plain-name_1.2~3");
    }

    #[test]
    fn render_inventory_lists_virtual_sensors() {
        let inventory = serde_json::json!({
            "sensors": [],
            "controls": [],
            "virtual": [{"id": "virtual/cpu_hot", "type": "max", "inputs": ["t1", "t2"]}]
        });
        let text = render_inventory(&inventory);
        assert!(text.contains("virtual/cpu_hot  [max]  t1, t2"));

        let no_virtual = serde_json::json!({"sensors": [], "controls": []});
        assert!(!render_inventory(&no_virtual).contains("virtual:"));
    }

    #[test]
    fn render_inventory_lists_labels_and_kinds() {
        let inventory = serde_json::json!({
            "sensors": [{"id": "hwmon/x/temp1", "label": "CPUTIN", "kind": "temp"}],
            "controls": [{"id": "hwmon/x/pwm1", "label": "x pwm1"}]
        });
        let text = render_inventory(&inventory);
        assert!(text.contains("hwmon/x/temp1  [temp]  CPUTIN"));
        assert!(text.contains("hwmon/x/pwm1  x pwm1"));
    }
}
