use tokio::sync::watch;

pub struct ShutdownSignal {
    rx: watch::Receiver<bool>,
}

pub struct ShutdownTrigger {
    tx: watch::Sender<bool>,
}

pub fn channel() -> (ShutdownTrigger, ShutdownSignal) {
    let (tx, rx) = watch::channel(false);
    (ShutdownTrigger { tx }, ShutdownSignal { rx })
}

impl ShutdownTrigger {
    pub fn fire(&self) {
        let _ = self.tx.send(true);
    }
}

impl Clone for ShutdownSignal {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.clone(),
        }
    }
}

impl ShutdownSignal {
    pub fn from_os() -> Self {
        let (trigger, signal) = channel();
        tokio::spawn(async move {
            wait_for_os_signal().await;
            trigger.fire();
        });
        signal
    }

    pub async fn wait(&mut self) {
        let _ = self.rx.wait_for(|fired| *fired).await;
    }
}

async fn wait_for_os_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fires_once_and_wait_resolves_for_two_clones() {
        let (trigger, mut signal_a) = channel();
        let mut signal_b = signal_a.clone();
        trigger.fire();
        signal_a.wait().await;
        signal_b.wait().await;
    }

    #[tokio::test]
    async fn wait_blocks_until_fired() {
        let (trigger, mut signal) = channel();
        let waited = tokio::spawn(async move {
            signal.wait().await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(!waited.is_finished());
        trigger.fire();
        waited.await.unwrap();
    }
}
