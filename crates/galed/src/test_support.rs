use gale_hw::{Backend, HwError, Id, Inventory};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
