use std::time::Duration;

const MAX_REPORT_LENGTH: usize = 96;

const MAX_DRAIN_ITERATIONS: usize = 32;
const MAX_QUIET_DRAIN_ITERATIONS: usize = 8;

pub trait HidTransport: Send {
    fn write(&mut self, data: &[u8]) -> Result<(), String>;
    fn read_timeout(&mut self, timeout_ms: i32) -> Result<Option<Vec<u8>>, String>;

    fn drain(&mut self) -> usize {
        0
    }

    fn drain_until_quiet(&mut self, _quiet: Duration) -> usize {
        self.drain()
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

pub struct HidapiTransport {
    device: hidapi::HidDevice,
}

impl HidapiTransport {
    pub fn new(device: hidapi::HidDevice) -> Self {
        Self { device }
    }
}

impl HidTransport for HidapiTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        let mut report = Vec::with_capacity(data.len() + 1);
        report.push(0x00);
        report.extend_from_slice(data);
        self.device
            .write(&report)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn read_timeout(&mut self, timeout_ms: i32) -> Result<Option<Vec<u8>>, String> {
        let mut buf = [0u8; MAX_REPORT_LENGTH];
        let read = self
            .device
            .read_timeout(&mut buf, timeout_ms)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            Ok(None)
        } else {
            Ok(Some(buf[..read].to_vec()))
        }
    }

    fn drain(&mut self) -> usize {
        self.discard_reports(0, MAX_DRAIN_ITERATIONS)
    }

    fn drain_until_quiet(&mut self, quiet: Duration) -> usize {
        let timeout_ms = quiet.as_millis().try_into().unwrap_or(i32::MAX);
        self.discard_reports(timeout_ms, MAX_QUIET_DRAIN_ITERATIONS)
    }
}

impl HidapiTransport {
    fn discard_reports(&mut self, timeout_ms: i32, max_reports: usize) -> usize {
        let mut buf = [0u8; MAX_REPORT_LENGTH];
        let mut discarded = 0;
        for _ in 0..max_reports {
            match self.device.read_timeout(&mut buf, timeout_ms) {
                Ok(0) | Err(_) => break,
                Ok(_) => discarded += 1,
            }
        }
        discarded
    }
}

#[cfg(any(test, feature = "testing"))]
pub struct FakeTransport {
    pub exchanges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    pub cursor: usize,
    pub writes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub drains: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub sleeps: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub quiet_drains: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub stale_reports: std::collections::HashMap<usize, usize>,
}

#[cfg(any(test, feature = "testing"))]
impl FakeTransport {
    pub fn new(exchanges: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Self {
        Self {
            exchanges,
            cursor: 0,
            writes: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            drains: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            sleeps: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            quiet_drains: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            stale_reports: std::collections::HashMap::new(),
        }
    }

    pub fn with_stale_reports(mut self, before_exchange: usize, count: usize) -> Self {
        self.stale_reports.insert(before_exchange, count);
        self
    }

    pub fn quiet_drain_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        self.quiet_drains.clone()
    }

    pub fn write_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        self.writes.clone()
    }

    pub fn drain_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        self.drains.clone()
    }

    pub fn sleep_counter(&self) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        self.sleeps.clone()
    }
}

#[cfg(any(test, feature = "testing"))]
fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(any(test, feature = "testing"))]
impl HidTransport for FakeTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        let Some((expected, _)) = self.exchanges.get(self.cursor) else {
            panic!(
                "unexpected write, no exchange scripted at cursor {}: got [{}]",
                self.cursor,
                hex(data)
            );
        };
        if expected.as_slice() != data {
            panic!(
                "write mismatch at cursor {}:\n  expected: [{}]\n  actual:   [{}]",
                self.cursor,
                hex(expected),
                hex(data)
            );
        }
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn read_timeout(&mut self, _timeout_ms: i32) -> Result<Option<Vec<u8>>, String> {
        let Some((_, response)) = self.exchanges.get(self.cursor).cloned() else {
            return Ok(None);
        };
        self.cursor += 1;
        Ok(response)
    }

    fn drain(&mut self) -> usize {
        self.drains
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.stale_reports.remove(&self.cursor).unwrap_or(0)
    }

    fn drain_until_quiet(&mut self, _quiet: Duration) -> usize {
        self.quiet_drains
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        0
    }

    fn sleep(&mut self, _duration: Duration) {
        self.sleeps
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_transport_returns_scripted_response_after_matching_write() {
        let mut transport = FakeTransport::new(vec![(vec![0x01, 0x02], Some(vec![0xaa, 0xbb]))]);
        transport.write(&[0x01, 0x02]).unwrap();
        assert_eq!(transport.read_timeout(100).unwrap(), Some(vec![0xaa, 0xbb]));
    }

    #[test]
    fn fake_transport_read_timeout_with_no_scripted_response_returns_none() {
        let mut transport = FakeTransport::new(vec![]);
        assert_eq!(transport.read_timeout(100).unwrap(), None);
    }

    #[test]
    #[should_panic(expected = "write mismatch")]
    fn fake_transport_panics_on_write_mismatch() {
        let mut transport = FakeTransport::new(vec![(vec![0x01], Some(vec![0xaa]))]);
        transport.write(&[0x02]).unwrap();
    }

    #[test]
    fn fake_transport_drain_reports_stale_reports_scripted_before_an_exchange_once() {
        let mut transport =
            FakeTransport::new(vec![(vec![0x01], Some(vec![0xaa]))]).with_stale_reports(0, 2);
        assert_eq!(transport.drain(), 2);
        assert_eq!(transport.drain(), 0);
    }

    #[test]
    fn fake_transport_advances_cursor_across_multiple_exchanges() {
        let mut transport =
            FakeTransport::new(vec![(vec![0x01], Some(vec![0xaa])), (vec![0x02], None)]);
        transport.write(&[0x01]).unwrap();
        assert_eq!(transport.read_timeout(0).unwrap(), Some(vec![0xaa]));
        transport.write(&[0x02]).unwrap();
        assert_eq!(transport.read_timeout(0).unwrap(), None);
    }
}
