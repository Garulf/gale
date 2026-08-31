use gale_hw::{Backend, HwError, Id, Inventory};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

enum Command {
    Enumerate(oneshot::Sender<Result<Inventory, HwError>>),
    ReadAll(oneshot::Sender<HashMap<Id, Option<f64>>>),
    SetDuty(Id, f64, oneshot::Sender<Result<(), HwError>>),
    Release(Id, oneshot::Sender<Result<(), HwError>>),
}

#[derive(Clone)]
pub struct BackendHandle {
    tx: mpsc::Sender<Command>,
}

fn thread_gone() -> HwError {
    HwError::Io {
        path: "backend".into(),
        message: "backend thread terminated".into(),
    }
}

impl BackendHandle {
    pub fn spawn(mut backend: Box<dyn Backend>) -> Self {
        let (tx, mut rx) = mpsc::channel::<Command>(32);
        std::thread::spawn(move || {
            while let Some(command) = rx.blocking_recv() {
                match command {
                    Command::Enumerate(reply) => {
                        let _ = reply.send(backend.enumerate());
                    }
                    Command::ReadAll(reply) => {
                        let _ = reply.send(backend.read_all());
                    }
                    Command::SetDuty(id, pct, reply) => {
                        let _ = reply.send(backend.set_duty(&id, pct));
                    }
                    Command::Release(id, reply) => {
                        let _ = reply.send(backend.release(&id));
                    }
                }
            }
        });
        Self { tx }
    }

    async fn send(&self, command: Command) -> Result<(), HwError> {
        self.tx.send(command).await.map_err(|_| thread_gone())
    }

    pub async fn enumerate(&self) -> Result<Inventory, HwError> {
        let (reply, rx) = oneshot::channel();
        self.send(Command::Enumerate(reply)).await?;
        rx.await.map_err(|_| thread_gone())?
    }

    pub async fn read_all(&self) -> HashMap<Id, Option<f64>> {
        let (reply, rx) = oneshot::channel();
        if self.send(Command::ReadAll(reply)).await.is_err() {
            return HashMap::new();
        }
        rx.await.unwrap_or_default()
    }

    pub async fn set_duty(&self, id: &str, pct: f64) -> Result<(), HwError> {
        let (reply, rx) = oneshot::channel();
        self.send(Command::SetDuty(id.to_string(), pct, reply))
            .await?;
        rx.await.map_err(|_| thread_gone())?
    }

    pub async fn release(&self, id: &str) -> Result<(), HwError> {
        let (reply, rx) = oneshot::channel();
        self.send(Command::Release(id.to_string(), reply)).await?;
        rx.await.map_err(|_| thread_gone())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_hw::{Backend, ControlInfo, HwError, Id, Inventory};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct FakeBackend {
        sets: Arc<AtomicU32>,
    }

    impl Backend for FakeBackend {
        fn name(&self) -> &str {
            "fake"
        }

        fn enumerate(&mut self) -> Result<Inventory, HwError> {
            Ok(Inventory {
                sensors: Vec::new(),
                controls: vec![ControlInfo {
                    id: "fake/p1".into(),
                    label: "p1".into(),
                }],
            })
        }

        fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
            [("fake/t1".to_string(), Some(50.0))].into()
        }

        fn set_duty(&mut self, id: &str, _pct: f64) -> Result<(), HwError> {
            if id != "fake/p1" {
                return Err(HwError::UnknownId(id.to_string()));
            }
            self.sets.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn release(&mut self, _id: &str) -> Result<(), HwError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn roundtrips_all_commands_through_backend_thread() {
        let sets = Arc::new(AtomicU32::new(0));
        let handle = BackendHandle::spawn(Box::new(FakeBackend { sets: sets.clone() }));
        let inventory = handle.enumerate().await.unwrap();
        assert_eq!(inventory.controls[0].id, "fake/p1");
        assert_eq!(handle.read_all().await["fake/t1"], Some(50.0));
        handle.set_duty("fake/p1", 40.0).await.unwrap();
        assert_eq!(sets.load(Ordering::SeqCst), 1);
        assert!(matches!(
            handle.set_duty("fake/nope", 1.0).await,
            Err(HwError::UnknownId(_))
        ));
        handle.release("fake/p1").await.unwrap();
    }

    #[tokio::test]
    async fn handle_is_clone_and_usable_concurrently() {
        let sets = Arc::new(AtomicU32::new(0));
        let handle = BackendHandle::spawn(Box::new(FakeBackend { sets: sets.clone() }));
        let h2 = handle.clone();
        let a = tokio::spawn(async move { h2.set_duty("fake/p1", 10.0).await });
        handle.set_duty("fake/p1", 20.0).await.unwrap();
        a.await.unwrap().unwrap();
        assert_eq!(sets.load(Ordering::SeqCst), 2);
    }
}
