use gale_core::config::{CorsairReleaseMode, GaleConfig};
use gale_hw::{Backend, Id};
use gale_hw_corsair::{CorsairBackend, ReleaseMode};
use gale_hw_nvidia::NvidiaBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn release_mode_from_config(mode: &CorsairReleaseMode) -> ReleaseMode {
    match mode {
        CorsairReleaseMode::PinFull => ReleaseMode::PinFull,
        CorsairReleaseMode::KeepLast => ReleaseMode::KeepLast,
        CorsairReleaseMode::Fixed { percent } => ReleaseMode::Fixed(*percent),
    }
}

fn corsair_release_mode_for_restore() -> ReleaseMode {
    let path = crate::paths::config_path();
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "failed to read config during restore, defaulting corsair release mode to keep_last");
            return ReleaseMode::KeepLast;
        }
    };
    match GaleConfig::from_toml(&contents) {
        Ok(config) => release_mode_from_config(&config.hardware.corsair.on_release),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "failed to parse config during restore, defaulting corsair release mode to keep_last");
            ReleaseMode::KeepLast
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: Id,
    pub backend_kind: String,
    pub hint: Option<(String, String)>,
}

pub fn default_path() -> PathBuf {
    crate::paths::runtime_dir().join("claims.json")
}

pub fn backend_kind_of(id: &str) -> String {
    id.split('/').next().unwrap_or("unknown").to_string()
}

pub fn write(path: &Path, entries: &[JournalEntry]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(entries).map_err(io::Error::other)?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, path)
}

pub fn load(path: &Path) -> Vec<JournalEntry> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_slice(&bytes) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "claim journal corrupt, ignoring");
            Vec::new()
        }
    }
}

fn restore_via_backends(mut backends: Vec<Box<dyn Backend>>, ids: &[Id]) -> usize {
    let mut restored = 0;
    for backend in backends.iter_mut() {
        let inventory = match backend.enumerate() {
            Ok(inventory) => inventory,
            Err(error) => {
                tracing::warn!(%error, "failed to enumerate backend during restore");
                continue;
            }
        };
        let owned: std::collections::HashSet<Id> =
            inventory.controls.into_iter().map(|c| c.id).collect();
        for id in ids {
            if !owned.contains(id) {
                continue;
            }
            match backend.release(id) {
                Ok(()) => restored += 1,
                Err(error) => {
                    tracing::warn!(%id, %error, "failed to release control during restore")
                }
            }
        }
    }
    restored
}

const SUPERIO_HINT_KIND: &str = "superio";

pub type HintRestorer = dyn Fn(&str, &str) -> Result<(), String>;

pub fn kind_has_restore_hint(kind: &str) -> bool {
    matches!(kind, "hwmon" | "superio")
}

pub fn restore_hint(kind_or_path: &str, value: &str) -> Result<(), String> {
    if kind_or_path == SUPERIO_HINT_KIND {
        return restore_superio_hint(value);
    }
    fs::write(kind_or_path, value).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn restore_superio_hint(value: &str) -> Result<(), String> {
    gale_hw_superio::restore::restore_on_hardware(value)
}

#[cfg(not(windows))]
fn restore_superio_hint(_value: &str) -> Result<(), String> {
    Err("superio hints can only be restored on Windows".to_string())
}

#[cfg(windows)]
fn superio_backends() -> Vec<Box<dyn Backend>> {
    match gale_hw_superio::probe() {
        Ok(backend) => vec![Box::new(backend) as Box<dyn Backend>],
        Err(_) => Vec::new(),
    }
}

fn backends_for_kind(kind: &str) -> Vec<Box<dyn Backend>> {
    match kind {
        "corsair" => CorsairBackend::open_all(corsair_release_mode_for_restore())
            .into_iter()
            .map(|backend| Box::new(backend) as Box<dyn Backend>)
            .collect(),
        "nvidia" => vec![Box::new(NvidiaBackend::new()) as Box<dyn Backend>],
        #[cfg(any(target_os = "linux", windows))]
        "cpu" => vec![Box::new(gale_hw_cpu::probe()) as Box<dyn Backend>],
        #[cfg(windows)]
        "superio" => superio_backends(),
        other => {
            tracing::warn!(
                backend_kind = other,
                "unknown backend kind in claim journal"
            );
            Vec::new()
        }
    }
}

pub fn run_restore(path: &Path) -> usize {
    run_restore_with(path, backends_for_kind, &|kind_or_path, value| {
        restore_hint(kind_or_path, value)
    })
}

fn run_restore_with(
    path: &Path,
    backend_factory: impl Fn(&str) -> Vec<Box<dyn Backend>>,
    hint_restorer: &HintRestorer,
) -> usize {
    let entries = load(path);
    if entries.is_empty() {
        return 0;
    }

    let mut restored = 0;
    let mut hintless: HashMap<String, Vec<Id>> = HashMap::new();

    for entry in &entries {
        match &entry.hint {
            Some((hint_kind_or_path, value)) => match hint_restorer(hint_kind_or_path, value) {
                Ok(()) => restored += 1,
                Err(error) => {
                    tracing::warn!(id = %entry.id, hint = %hint_kind_or_path, %error, "failed to restore control via hint")
                }
            },
            None => hintless
                .entry(entry.backend_kind.clone())
                .or_default()
                .push(entry.id.clone()),
        }
    }

    for (kind, ids) in &hintless {
        restored += restore_via_backends(backend_factory(kind), ids);
    }

    if restored == entries.len() {
        if let Err(error) = fs::remove_file(path) {
            tracing::warn!(%error, path = %path.display(), "failed to remove claim journal after restore");
        }
    }

    restored
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_hw::{ControlInfo, HwError, Inventory};
    use std::sync::{Arc, Mutex};

    fn fs_hint_restorer(path: &str, value: &str) -> Result<(), String> {
        fs::write(path, value).map_err(|error| error.to_string())
    }

    struct FakeRestoreBackend {
        controls: Vec<Id>,
        fail_enumerate: bool,
        fail_release_for: Option<Id>,
        released: Arc<Mutex<Vec<Id>>>,
    }

    impl Backend for FakeRestoreBackend {
        fn name(&self) -> &str {
            "fake-restore"
        }

        fn enumerate(&mut self) -> Result<Inventory, HwError> {
            if self.fail_enumerate {
                return Err(HwError::Io {
                    path: "fake".to_string(),
                    message: "boom".to_string(),
                });
            }
            Ok(Inventory {
                sensors: Vec::new(),
                controls: self
                    .controls
                    .iter()
                    .map(|id| ControlInfo {
                        id: id.clone(),
                        label: id.clone(),
                    })
                    .collect(),
            })
        }

        fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
            HashMap::new()
        }

        fn set_duty(&mut self, _id: &str, _pct: f64) -> Result<(), HwError> {
            Ok(())
        }

        fn release(&mut self, id: &str) -> Result<(), HwError> {
            if self.fail_release_for.as_deref() == Some(id) {
                return Err(HwError::UnknownId(id.to_string()));
            }
            self.released.lock().unwrap().push(id.to_string());
            Ok(())
        }
    }

    #[test]
    fn run_restore_releases_matching_hintless_entry_and_clears_journal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![JournalEntry {
            id: "corsair/abc/fan1".to_string(),
            backend_kind: "corsair".to_string(),
            hint: None,
        }];
        write(&path, &entries).unwrap();

        let released = Arc::new(Mutex::new(Vec::new()));
        let released_for_factory = released.clone();
        let restored = run_restore_with(
            &path,
            move |kind| {
                assert_eq!(kind, "corsair");
                vec![Box::new(FakeRestoreBackend {
                    controls: vec!["corsair/abc/fan1".to_string()],
                    fail_enumerate: false,
                    fail_release_for: None,
                    released: released_for_factory.clone(),
                }) as Box<dyn Backend>]
            },
            &fs_hint_restorer,
        );

        assert_eq!(restored, 1);
        assert_eq!(
            released.lock().unwrap().as_slice(),
            ["corsair/abc/fan1".to_string()]
        );
        assert!(!path.exists());
    }

    #[test]
    fn run_restore_leaves_journal_when_hintless_release_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![JournalEntry {
            id: "corsair/abc/fan1".to_string(),
            backend_kind: "corsair".to_string(),
            hint: None,
        }];
        write(&path, &entries).unwrap();

        let released = Arc::new(Mutex::new(Vec::new()));
        let restored = run_restore_with(
            &path,
            move |_kind| {
                vec![Box::new(FakeRestoreBackend {
                    controls: vec!["corsair/abc/fan1".to_string()],
                    fail_enumerate: false,
                    fail_release_for: Some("corsair/abc/fan1".to_string()),
                    released: released.clone(),
                }) as Box<dyn Backend>]
            },
            &fs_hint_restorer,
        );

        assert_eq!(restored, 0);
        assert!(path.exists());
    }

    #[test]
    fn run_restore_leaves_journal_when_hintless_id_not_enumerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![JournalEntry {
            id: "corsair/abc/fan1".to_string(),
            backend_kind: "corsair".to_string(),
            hint: None,
        }];
        write(&path, &entries).unwrap();

        let restored = run_restore_with(
            &path,
            |_kind| {
                vec![Box::new(FakeRestoreBackend {
                    controls: vec!["corsair/other/fan9".to_string()],
                    fail_enumerate: false,
                    fail_release_for: None,
                    released: Arc::new(Mutex::new(Vec::new())),
                }) as Box<dyn Backend>]
            },
            &fs_hint_restorer,
        );

        assert_eq!(restored, 0);
        assert!(path.exists());
    }

    #[test]
    fn run_restore_leaves_journal_and_warns_when_enumerate_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![JournalEntry {
            id: "corsair/abc/fan1".to_string(),
            backend_kind: "corsair".to_string(),
            hint: None,
        }];
        write(&path, &entries).unwrap();

        let restored = run_restore_with(
            &path,
            |_kind| {
                vec![Box::new(FakeRestoreBackend {
                    controls: vec!["corsair/abc/fan1".to_string()],
                    fail_enumerate: true,
                    fail_release_for: None,
                    released: Arc::new(Mutex::new(Vec::new())),
                }) as Box<dyn Backend>]
            },
            &fs_hint_restorer,
        );

        assert_eq!(restored, 0);
        assert!(path.exists());
    }

    #[test]
    fn write_then_load_round_trips_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![
            JournalEntry {
                id: "hwmon/nct6798/pwm1".to_string(),
                backend_kind: "hwmon".to_string(),
                hint: Some(("/sys/.../pwm1_enable".to_string(), "5".to_string())),
            },
            JournalEntry {
                id: "corsair/abc/fan1".to_string(),
                backend_kind: "corsair".to_string(),
                hint: None,
            },
        ];
        write(&path, &entries).unwrap();
        assert_eq!(load(&path), entries);
    }

    #[test]
    fn load_missing_journal_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        assert_eq!(load(&path), Vec::new());
    }

    #[test]
    fn load_corrupt_journal_returns_empty_and_warns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        fs::write(&path, b"not json").unwrap();
        assert_eq!(load(&path), Vec::new());
    }

    #[test]
    fn release_mode_from_config_maps_all_three_variants() {
        assert_eq!(
            release_mode_from_config(&CorsairReleaseMode::PinFull),
            ReleaseMode::PinFull
        );
        assert_eq!(
            release_mode_from_config(&CorsairReleaseMode::KeepLast),
            ReleaseMode::KeepLast
        );
        assert_eq!(
            release_mode_from_config(&CorsairReleaseMode::Fixed { percent: 42 }),
            ReleaseMode::Fixed(42)
        );
    }

    #[test]
    fn backend_kind_of_derives_from_id_prefix() {
        assert_eq!(backend_kind_of("hwmon/nct6798/pwm1"), "hwmon");
        assert_eq!(backend_kind_of("corsair/abc/fan1"), "corsair");
        assert_eq!(backend_kind_of("nvidia/0/fan0"), "nvidia");
    }

    #[test]
    fn run_restore_on_missing_journal_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        assert_eq!(run_restore(&path), 0);
    }

    #[test]
    fn run_restore_writes_hint_value_and_deletes_journal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let enable_path = dir.path().join("pwm1_enable");
        fs::write(&enable_path, "1").unwrap();
        let entries = vec![JournalEntry {
            id: "hwmon/nct6798/pwm1".to_string(),
            backend_kind: "hwmon".to_string(),
            hint: Some((enable_path.display().to_string(), "5".to_string())),
        }];
        write(&path, &entries).unwrap();

        let restored = run_restore(&path);

        assert_eq!(restored, 1);
        assert_eq!(fs::read_to_string(&enable_path).unwrap(), "5");
        assert!(!path.exists());
    }

    #[test]
    fn kind_has_restore_hint_matches_hwmon_and_superio_only() {
        assert!(kind_has_restore_hint("hwmon"));
        assert!(kind_has_restore_hint("superio"));
        assert!(!kind_has_restore_hint("corsair"));
    }

    #[cfg(not(windows))]
    #[test]
    fn restore_hint_for_superio_fails_with_windows_message_on_non_windows() {
        let error = restore_hint("superio", "nct6798d:pwm1:42:7f").unwrap_err();
        assert!(error.contains("Windows"));
    }

    #[test]
    fn run_restore_with_routes_superio_hint_to_the_injected_restorer_without_touching_the_filesystem(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![JournalEntry {
            id: "superio/nct6798d/pwm1".to_string(),
            backend_kind: "superio".to_string(),
            hint: Some(("superio".to_string(), "nct6798d:pwm1:42:7f".to_string())),
        }];
        write(&path, &entries).unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_for_restorer = seen.clone();
        let restored = run_restore_with(&path, |_kind| Vec::new(), &move |kind_or_path, value| {
            seen_for_restorer
                .lock()
                .unwrap()
                .push((kind_or_path.to_string(), value.to_string()));
            Ok(())
        });

        assert_eq!(restored, 1);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [("superio".to_string(), "nct6798d:pwm1:42:7f".to_string())]
        );
        assert!(!dir.path().join("superio").exists());
        assert!(!path.exists());
    }

    #[test]
    fn run_restore_with_leaves_journal_when_superio_restorer_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![JournalEntry {
            id: "superio/nct6798d/pwm1".to_string(),
            backend_kind: "superio".to_string(),
            hint: Some(("superio".to_string(), "nct6798d:pwm1:42:7f".to_string())),
        }];
        write(&path, &entries).unwrap();

        let restored = run_restore_with(&path, |_kind| Vec::new(), &|_kind_or_path, _value| {
            Err("boom".to_string())
        });

        assert_eq!(restored, 0);
        assert!(path.exists());
    }

    #[test]
    fn run_restore_leaves_journal_when_hint_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claims.json");
        let entries = vec![JournalEntry {
            id: "hwmon/nct6798/pwm1".to_string(),
            backend_kind: "hwmon".to_string(),
            hint: Some((
                dir.path()
                    .join("missing-dir")
                    .join("pwm1_enable")
                    .display()
                    .to_string(),
                "5".to_string(),
            )),
        }];
        write(&path, &entries).unwrap();

        let restored = run_restore(&path);

        assert_eq!(restored, 0);
        assert!(path.exists());
    }
}
