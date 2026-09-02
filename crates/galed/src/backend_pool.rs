use crate::backend_handle::BackendHandle;
use gale_hw::{HwError, Id, Inventory};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

const ENUMERATE_TIMEOUT: Duration = Duration::from_millis(5000);
const READ_ALL_TIMEOUT: Duration = Duration::from_millis(2000);
const WRITE_TIMEOUT: Duration = Duration::from_millis(5000);

pub struct BackendPool {
    handles: Vec<BackendHandle>,
    owners: RwLock<HashMap<Id, usize>>,
}

fn timed_out(id: &str) -> HwError {
    HwError::Io {
        path: id.to_string(),
        message: "backend timed out".into(),
    }
}

impl BackendPool {
    pub fn new(handles: Vec<BackendHandle>) -> Self {
        Self {
            handles,
            owners: RwLock::new(HashMap::new()),
        }
    }

    pub async fn enumerate(&self) -> Inventory {
        let previous_owners = self.owners.read().unwrap().clone();
        let mut merged = Inventory::default();
        let mut owners = HashMap::new();
        for (index, handle) in self.handles.iter().enumerate() {
            match tokio::time::timeout(ENUMERATE_TIMEOUT, handle.enumerate()).await {
                Ok(Ok(inventory)) => {
                    for control in &inventory.controls {
                        owners.insert(control.id.clone(), index);
                    }
                    merged.sensors.extend(inventory.sensors);
                    merged.controls.extend(inventory.controls);
                }
                Ok(Err(error)) => {
                    tracing::warn!(backend = index, %error, "backend enumerate failed, skipping");
                    for (id, owner) in &previous_owners {
                        if *owner == index {
                            owners.insert(id.clone(), index);
                        }
                    }
                }
                Err(_) => {
                    tracing::warn!(backend = index, "backend enumerate timed out, skipping");
                    for (id, owner) in &previous_owners {
                        if *owner == index {
                            owners.insert(id.clone(), index);
                        }
                    }
                }
            }
        }
        *self.owners.write().unwrap() = owners;
        merged
    }

    pub async fn read_all(&self) -> HashMap<Id, Option<f64>> {
        let reads = self
            .handles
            .iter()
            .enumerate()
            .map(|(index, handle)| async move {
                (
                    index,
                    tokio::time::timeout(READ_ALL_TIMEOUT, handle.read_all()).await,
                )
            });
        let results = futures::future::join_all(reads).await;
        let mut merged = HashMap::new();
        for (index, result) in results {
            match result {
                Ok(values) => merged.extend(values),
                Err(_) => {
                    tracing::warn!(
                        backend = index,
                        "backend read_all timed out, skipping this cycle"
                    );
                }
            }
        }
        merged
    }

    fn owner_of(&self, id: &str) -> Result<usize, HwError> {
        let owners = self.owners.read().unwrap();
        if let Some(index) = owners.get(id).copied() {
            return Ok(index);
        }
        if owners.is_empty() && self.handles.len() == 1 {
            return Ok(0);
        }
        Err(HwError::UnknownId(id.to_string()))
    }

    pub async fn set_duty(&self, id: &str, pct: f64) -> Result<(), HwError> {
        let index = self.owner_of(id)?;
        match tokio::time::timeout(WRITE_TIMEOUT, self.handles[index].set_duty(id, pct)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(backend = index, %id, "set_duty timed out");
                Err(timed_out(id))
            }
        }
    }

    pub async fn release(&self, id: &str) -> Result<(), HwError> {
        let index = self.owner_of(id)?;
        match tokio::time::timeout(WRITE_TIMEOUT, self.handles[index].release(id)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(backend = index, %id, "release timed out");
                Err(timed_out(id))
            }
        }
    }

    pub fn blocking_release(&self, id: &str) {
        if let Ok(index) = self.owner_of(id) {
            self.handles[index].blocking_release(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_hw::{Backend, ControlInfo, SensorInfo, SensorKind};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct FakeBackend {
        name: &'static str,
        fail_enumerate: Arc<AtomicBool>,
        block_read_all: bool,
    }

    impl FakeBackend {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                fail_enumerate: Arc::new(AtomicBool::new(false)),
                block_read_all: false,
            }
        }

        fn with_fail_flag(name: &'static str, fail_enumerate: Arc<AtomicBool>) -> Self {
            Self {
                name,
                fail_enumerate,
                block_read_all: false,
            }
        }

        fn blocking(name: &'static str) -> Self {
            Self {
                name,
                fail_enumerate: Arc::new(AtomicBool::new(false)),
                block_read_all: true,
            }
        }
    }

    impl Backend for FakeBackend {
        fn name(&self) -> &str {
            self.name
        }

        fn enumerate(&mut self) -> Result<Inventory, HwError> {
            if self.fail_enumerate.load(Ordering::SeqCst) {
                return Err(HwError::Io {
                    path: self.name.into(),
                    message: "boom".into(),
                });
            }
            Ok(Inventory {
                sensors: vec![SensorInfo {
                    id: format!("{}/t1", self.name),
                    label: "t1".into(),
                    kind: SensorKind::Temp,
                }],
                controls: vec![ControlInfo {
                    id: format!("{}/p1", self.name),
                    label: "p1".into(),
                }],
            })
        }

        fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
            if self.block_read_all {
                std::thread::sleep(Duration::from_secs(10));
            }
            [(format!("{}/t1", self.name), Some(1.0))].into()
        }

        fn set_duty(&mut self, _id: &str, _pct: f64) -> Result<(), HwError> {
            Ok(())
        }

        fn release(&mut self, _id: &str) -> Result<(), HwError> {
            Ok(())
        }
    }

    fn pool_of(backends: Vec<FakeBackend>) -> BackendPool {
        let handles = backends
            .into_iter()
            .map(|backend| BackendHandle::spawn(Box::new(backend)))
            .collect();
        BackendPool::new(handles)
    }

    #[tokio::test]
    async fn merges_inventory_and_readings_across_handles() {
        let pool = pool_of(vec![FakeBackend::new("a"), FakeBackend::new("b")]);
        let inventory = pool.enumerate().await;
        let sensor_ids: Vec<_> = inventory.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(sensor_ids, vec!["a/t1", "b/t1"]);
        assert_eq!(inventory.controls.len(), 2);
        let values = pool.read_all().await;
        assert_eq!(values["a/t1"], Some(1.0));
        assert_eq!(values["b/t1"], Some(1.0));
    }

    #[tokio::test]
    async fn routes_writes_to_owning_handle_and_rejects_unknown() {
        let pool = pool_of(vec![FakeBackend::new("a"), FakeBackend::new("b")]);
        pool.enumerate().await;
        pool.set_duty("b/p1", 40.0).await.unwrap();
        pool.release("b/p1").await.unwrap();
        assert!(matches!(
            pool.set_duty("ghost/p1", 1.0).await,
            Err(HwError::UnknownId(_))
        ));
        assert!(matches!(
            pool.release("ghost/p1").await,
            Err(HwError::UnknownId(_))
        ));
    }

    #[tokio::test]
    async fn handle_whose_enumerate_fails_keeps_prior_ownership_routable() {
        let fail_b = Arc::new(AtomicBool::new(false));
        let pool = pool_of(vec![
            FakeBackend::new("a"),
            FakeBackend::with_fail_flag("b", fail_b.clone()),
        ]);
        pool.enumerate().await;
        pool.set_duty("b/p1", 40.0).await.unwrap();

        fail_b.store(true, Ordering::SeqCst);
        let inventory = pool.enumerate().await;

        assert!(inventory.controls.iter().any(|c| c.id == "a/p1"));
        pool.release("b/p1").await.unwrap();
    }

    #[tokio::test]
    async fn slow_handle_does_not_block_other_handles_read_all() {
        let pool = pool_of(vec![
            FakeBackend::blocking("slow"),
            FakeBackend::new("fast"),
        ]);
        let values = tokio::time::timeout(Duration::from_secs(4), pool.read_all())
            .await
            .expect("pool.read_all() should honor its own per-handle timeout");
        assert_eq!(values.get("fast/t1"), Some(&Some(1.0)));
        assert!(!values.contains_key("slow/t1"));
    }

    #[tokio::test]
    async fn multiple_wedged_handles_time_out_concurrently_not_serially() {
        let pool = pool_of(vec![
            FakeBackend::blocking("slow-a"),
            FakeBackend::blocking("slow-b"),
            FakeBackend::new("fast"),
        ]);
        let started = std::time::Instant::now();
        let values = pool.read_all().await;
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(3500),
            "read_all took {elapsed:?}, expected roughly one 2s timeout, not one per wedged handle"
        );
        assert_eq!(values.get("fast/t1"), Some(&Some(1.0)));
        assert!(!values.contains_key("slow-a/t1"));
        assert!(!values.contains_key("slow-b/t1"));
    }

    #[tokio::test]
    async fn single_handle_with_populated_ownership_rejects_unknown_id() {
        let pool = pool_of(vec![FakeBackend::new("a")]);
        pool.enumerate().await;
        assert!(matches!(
            pool.set_duty("ghost/p1", 1.0).await,
            Err(HwError::UnknownId(_))
        ));
        assert!(matches!(
            pool.release("ghost/p1").await,
            Err(HwError::UnknownId(_))
        ));
    }
}
