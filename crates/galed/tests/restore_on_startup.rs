#![cfg(target_os = "linux")]

use galed::claims_journal::{self, JournalEntry};
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::os::unix::fs::MetadataExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct DaemonGuard(Child);

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn write_file(path: &std::path::Path, contents: &str) {
    let mut file = fs::File::create(path).unwrap();
    file.write_all(contents.as_bytes()).unwrap();
}

#[test]
fn leftover_journal_is_restored_before_normal_startup_claims_the_control() {
    let workdir = tempfile::tempdir().unwrap();

    let chip0 = workdir.path().join("hwmon/hwmon0");
    fs::create_dir_all(&chip0).unwrap();
    write_file(&chip0.join("name"), "nct6798\n");
    write_file(&chip0.join("temp1_input"), "45000\n");
    write_file(&chip0.join("temp1_label"), "CPUTIN\n");
    write_file(&chip0.join("fan1_input"), "1200\n");
    write_file(&chip0.join("pwm1"), "128\n");
    write_file(&chip0.join("pwm1_mode"), "1\n");
    let enable_path = chip0.join("pwm1_enable");
    write_file(&enable_path, "1\n");

    let runtime_dir = workdir.path().join("run");
    fs::create_dir_all(&runtime_dir).unwrap();
    let journal_path = runtime_dir.join("claims.json");
    claims_journal::write(
        &journal_path,
        &[JournalEntry {
            id: "hwmon/nct6798/pwm1".to_string(),
            backend_kind: "hwmon".to_string(),
            hint: Some((enable_path.display().to_string(), "5".to_string())),
        }],
    )
    .unwrap();
    let seed_ino = fs::metadata(&journal_path).unwrap().ino();

    let port = free_port();
    let config_path = workdir.path().join("config.toml");
    write_file(
        &config_path,
        &format!(
            r#"
tick_interval_ms = 50
active_profile = "default"

[api]
bind = "127.0.0.1:{port}"

[profiles.default.curves.cpu]
type = "point"
sensor = "hwmon/nct6798/temp1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.default.assignments]
"hwmon/nct6798/pwm1" = "cpu"
"#
        ),
    );

    let daemon = DaemonGuard(
        Command::new(env!("CARGO_BIN_EXE_galed"))
            .env("GALE_HWMON_ROOT", workdir.path().join("hwmon"))
            .env("GALE_CONFIG", &config_path)
            .env("GALE_RUNTIME_DIR", &runtime_dir)
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut written_by_daemon = None;
    while Instant::now() < deadline {
        if let Ok(metadata) = fs::metadata(&journal_path) {
            if metadata.ino() != seed_ino {
                let entries = claims_journal::load(&journal_path);
                if let Some(entry) = entries.iter().find(|e| e.id == "hwmon/nct6798/pwm1") {
                    if let Some(hint) = &entry.hint {
                        written_by_daemon = Some(hint.clone());
                        break;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(daemon);

    let (_, saved_value) =
        written_by_daemon.expect("daemon never wrote its own journal entry for pwm1");
    assert_eq!(
        saved_value, "5",
        "startup should restore the leftover journal's hint before claiming, \
         so the daemon's own saved value reflects the restored state, not the stale one it \
         found on disk"
    );
}
