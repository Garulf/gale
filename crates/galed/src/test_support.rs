use gale_hw::{Backend, HwError, Id, Inventory};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex, MutexGuard};

pub static ENV_LOCK: Mutex<()> = Mutex::new(());

pub fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Default)]
pub struct EnvVarGuard {
    keys: Vec<&'static str>,
}

impl EnvVarGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: &'static str, value: impl AsRef<OsStr>) {
        unsafe {
            std::env::set_var(key, value);
        }
        self.keys.push(key);
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for key in &self.keys {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }
}

#[derive(Default)]
pub struct Recorded {
    pub duties: HashMap<Id, f64>,
    pub released: Vec<Id>,
    pub fail_ids: Vec<Id>,
    pub fail_release_ids: Vec<Id>,
    pub sensors: HashMap<Id, Option<f64>>,
}

pub struct RecordingBackend {
    pub state: Arc<Mutex<Recorded>>,
}

impl Backend for RecordingBackend {
    fn name(&self) -> &str {
        "recording"
    }

    fn enumerate(&mut self) -> Result<Inventory, HwError> {
        Ok(Inventory::default())
    }

    fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
        self.state.lock().unwrap().sensors.clone()
    }

    fn set_duty(&mut self, id: &str, pct: f64) -> Result<(), HwError> {
        let mut state = self.state.lock().unwrap();
        if state.fail_ids.iter().any(|f| f == id) {
            return Err(HwError::UnknownId(id.to_string()));
        }
        state.duties.insert(id.to_string(), pct);
        Ok(())
    }

    fn release(&mut self, id: &str) -> Result<(), HwError> {
        let mut state = self.state.lock().unwrap();
        if state.fail_release_ids.iter().any(|f| f == id) {
            return Err(HwError::UnknownId(id.to_string()));
        }
        state.released.push(id.to_string());
        Ok(())
    }
}
