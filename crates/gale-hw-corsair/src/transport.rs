pub trait HidTransport: Send {
    fn write(&mut self, data: &[u8]) -> Result<(), String>;
    fn read_timeout(&mut self, timeout_ms: i32) -> Result<Option<Vec<u8>>, String>;
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
        let mut buf = [0u8; 64];
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
}

#[cfg(any(test, feature = "testing"))]
pub struct FakeTransport {
    pub exchanges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    pub cursor: usize,
}

#[cfg(any(test, feature = "testing"))]
impl FakeTransport {
    pub fn new(exchanges: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Self {
        Self {
            exchanges,
            cursor: 0,
        }
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
        Ok(())
    }

    fn read_timeout(&mut self, _timeout_ms: i32) -> Result<Option<Vec<u8>>, String> {
        let Some((_, response)) = self.exchanges.get(self.cursor).cloned() else {
            return Ok(None);
        };
        self.cursor += 1;
        Ok(response)
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
    fn fake_transport_advances_cursor_across_multiple_exchanges() {
        let mut transport =
            FakeTransport::new(vec![(vec![0x01], Some(vec![0xaa])), (vec![0x02], None)]);
        transport.write(&[0x01]).unwrap();
        assert_eq!(transport.read_timeout(0).unwrap(), Some(vec![0xaa]));
        transport.write(&[0x02]).unwrap();
        assert_eq!(transport.read_timeout(0).unwrap(), None);
    }
}
