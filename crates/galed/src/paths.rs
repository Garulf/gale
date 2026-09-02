use std::path::PathBuf;

fn program_data() -> PathBuf {
    std::env::var("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\ProgramData"))
}

pub fn runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("GALE_RUNTIME_DIR") {
        return PathBuf::from(dir);
    }
    if cfg!(windows) {
        program_data().join("Gale").join("run")
    } else {
        PathBuf::from("/run/gale")
    }
}

pub fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("GALE_CONFIG") {
        return PathBuf::from(path);
    }
    if cfg!(windows) {
        program_data().join("Gale").join("config.toml")
    } else {
        PathBuf::from("/etc/gale/config.toml")
    }
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    if let Some(parent) = config_path().parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) use crate::test_support::ENV_LOCK;

    #[test]
    fn runtime_dir_uses_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("GALE_RUNTIME_DIR", "/tmp/gale-test-runtime");
        }
        assert_eq!(runtime_dir(), PathBuf::from("/tmp/gale-test-runtime"));
        unsafe {
            std::env::remove_var("GALE_RUNTIME_DIR");
        }
    }

    #[test]
    fn config_path_uses_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("GALE_CONFIG", "/tmp/gale-test-config.toml");
        }
        assert_eq!(config_path(), PathBuf::from("/tmp/gale-test-config.toml"));
        unsafe {
            std::env::remove_var("GALE_CONFIG");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn runtime_dir_default_is_run_gale_on_linux() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("GALE_RUNTIME_DIR");
        }
        assert_eq!(runtime_dir(), PathBuf::from("/run/gale"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn config_path_default_is_etc_gale_on_linux() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("GALE_CONFIG");
        }
        assert_eq!(config_path(), PathBuf::from("/etc/gale/config.toml"));
    }

    #[cfg(windows)]
    #[test]
    fn runtime_dir_default_ends_with_programdata_gale_run() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("GALE_RUNTIME_DIR");
        }
        assert!(runtime_dir().ends_with("Gale/run") || runtime_dir().ends_with(r"Gale\run"));
    }

    #[cfg(windows)]
    #[test]
    fn config_path_default_ends_with_programdata_gale_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("GALE_CONFIG");
        }
        assert!(
            config_path().ends_with("Gale/config.toml")
                || config_path().ends_with(r"Gale\config.toml")
        );
    }
}
