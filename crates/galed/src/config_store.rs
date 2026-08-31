use gale_core::config::{ConfigError, GaleConfig};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct ConfigStore {
    path: PathBuf,
    last_saved: Mutex<Option<Instant>>,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path, last_saved: Mutex::new(None) }
    }

    pub fn default_path() -> PathBuf {
        std::env::var("GALE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/etc/gale/config.toml"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<GaleConfig, ConfigError> {
        match std::fs::read_to_string(&self.path) {
            Ok(contents) => GaleConfig::from_toml(&contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(GaleConfig::default_config())
            }
            Err(error) => Err(ConfigError::Parse(error.to_string())),
        }
    }

    pub fn save(&self, config: &GaleConfig) -> Result<(), ConfigError> {
        let rendered = config.to_toml()?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ConfigError::Serialize(e.to_string()))?;
        }
        std::fs::write(&self.path, rendered).map_err(|e| ConfigError::Serialize(e.to_string()))?;
        *self.last_saved.lock().unwrap() = Some(Instant::now());
        Ok(())
    }

    pub fn recently_saved(&self, within: Duration) -> bool {
        self.last_saved
            .lock()
            .unwrap()
            .is_some_and(|instant| instant.elapsed() <= within)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_core::config::GaleConfig;

    #[test]
    fn missing_file_loads_default_config() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("config.toml"));
        let config = store.load().unwrap();
        assert_eq!(config.active_profile, "default");
        assert!(config.profiles.contains_key("default"));
        assert_eq!(config.tick_interval_ms, 1000);
    }

    #[test]
    fn save_then_load_round_trips_and_marks_recent() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("nested").join("config.toml"));
        let mut config = GaleConfig::default_config();
        config.tick_interval_ms = 250;
        assert!(!store.recently_saved(std::time::Duration::from_secs(5)));
        store.save(&config).unwrap();
        assert!(store.recently_saved(std::time::Duration::from_secs(5)));
        assert_eq!(store.load().unwrap(), config);
    }

    #[test]
    fn invalid_toml_is_a_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not toml [").unwrap();
        let store = ConfigStore::new(path);
        assert!(store.load().is_err());
    }
}
