use gale_hw::{Backend, HwError, Id, Inventory};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

enum Command {
    Enumerate(oneshot::Sender<Result<Inventory, HwError>>),
    ReadAll(oneshot::Sender<HashMap<Id, Option<f64>>>),
    SetDuty(Id, f64, oneshot::Sender<Result<(), HwError>>),
    Release(Id, oneshot::Sender<Result<(), HwError>>),
    RestoreHint(Id, oneshot::Sender<Option<(String, String)>>),
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
                    Command::RestoreHint(id, reply) => {
                        let _ = reply.send(backend.restore_hint(&id));
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

    pub async fn restore_hint(&self, id: &str) -> Option<(String, String)> {
        let (reply, rx) = oneshot::channel();
        if self
            .send(Command::RestoreHint(id.to_string(), reply))
            .await
            .is_err()
        {
            return None;
        }
        rx.await.ok().flatten()
    }

    pub fn blocking_release(&self, id: &str) {
        let (reply, _rx) = oneshot::channel();
        if self
            .tx
            .try_send(Command::Release(id.to_string(), reply))
            .is_err()
        {
            eprintln!(
                "gale: blocking_release could not queue release for {id}, backend busy or gone"
            );
        }
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

        fn restore_hint(&self, id: &str) -> Option<(String, String)> {
            (id == "fake/p1").then(|| ("fake/p1_enable".to_string(), "1".to_string()))
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
        assert_eq!(
            handle.restore_hint("fake/p1").await,
            Some(("fake/p1_enable".to_string(), "1".to_string()))
        );
        assert_eq!(handle.restore_hint("fake/nope").await, None);
        handle.release("fake/p1").await.unwrap();
    }

    struct WedgedBackend {
        started: Arc<AtomicU32>,
    }

    impl Backend for WedgedBackend {
        fn name(&self) -> &str {
            "wedged"
        }

        fn enumerate(&mut self) -> Result<Inventory, HwError> {
            Ok(Inventory::default())
        }

        fn read_all(&mut self) -> HashMap<Id, Option<f64>> {
            self.started.fetch_add(1, Ordering::SeqCst);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }

        fn set_duty(&mut self, _id: &str, _pct: f64) -> Result<(), HwError> {
            Ok(())
        }

        fn release(&mut self, _id: &str) -> Result<(), HwError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn blocking_release_returns_promptly_when_queue_is_full() {
        let started = Arc::new(AtomicU32::new(0));
        let handle = BackendHandle::spawn(Box::new(WedgedBackend {
            started: started.clone(),
        }));

        let wedging = handle.clone();
        tokio::spawn(async move {
            let _ = wedging.read_all().await;
        });
        while started.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }

        for _ in 0..40 {
            let filler = handle.clone();
            tokio::spawn(async move {
                let _ = filler.read_all().await;
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            handle.blocking_release("fake/p1");
        })
        .await;
        assert!(
            outcome.is_ok(),
            "blocking_release must not hang when the command channel is full"
        );
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
