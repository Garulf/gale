pub mod backend;
pub mod decode;
#[cfg(windows)]
pub mod pawnio;

use gale_pawnio::PawnIoError;

pub const FEATURE: &str = "AMD CPU temperature";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmdCpuStatus {
    PawnIo(PawnIoError),
    NotAmd,
    Unsupported(String),
    Io(String),
}

impl AmdCpuStatus {
    pub fn warning_text(&self) -> Option<String> {
        match self {
            AmdCpuStatus::PawnIo(error) => Some(error.warning_text(FEATURE)),
            AmdCpuStatus::NotAmd | AmdCpuStatus::Unsupported(_) => None,
            AmdCpuStatus::Io(message) => Some(format!("{FEATURE} is unavailable: {message}")),
        }
    }
}

#[cfg(windows)]
pub fn probe() -> Result<backend::AmdCpuBackend, AmdCpuStatus> {
    pawnio::probe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_driver_and_io_failures_produce_warnings() {
        assert_eq!(AmdCpuStatus::NotAmd.warning_text(), None);
        assert_eq!(
            AmdCpuStatus::Unsupported("family 0x15".into()).warning_text(),
            None
        );
        assert!(AmdCpuStatus::PawnIo(PawnIoError::NotInstalled)
            .warning_text()
            .unwrap()
            .starts_with(FEATURE));
        assert!(AmdCpuStatus::Io("x".into())
            .warning_text()
            .unwrap()
            .contains("x"));
    }
}
