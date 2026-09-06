pub mod backend;
pub mod boards;
#[cfg(windows)]
pub mod pawnio;
pub mod protocol;

use gale_pawnio::PawnIoError;

pub const FEATURE: &str = "Motherboard embedded controller sensors";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EcStatus {
    NotAsus(String),
    UnknownBoard(String),
    PawnIo(PawnIoError),
    Io(String),
}

impl EcStatus {
    pub fn warning_text(&self) -> Option<String> {
        match self {
            EcStatus::NotAsus(_) | EcStatus::UnknownBoard(_) => None,
            EcStatus::PawnIo(error) => Some(error.warning_text(FEATURE)),
            EcStatus::Io(message) => Some(format!("{FEATURE} are unavailable: {message}")),
        }
    }
}

#[cfg(windows)]
pub fn probe() -> Result<backend::AsusEcBackend, EcStatus> {
    pawnio::probe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_boards_stay_silent_and_driver_failures_warn() {
        assert_eq!(EcStatus::UnknownBoard("X".into()).warning_text(), None);
        assert_eq!(EcStatus::NotAsus("MSI".into()).warning_text(), None);
        assert!(EcStatus::PawnIo(PawnIoError::NotInstalled)
            .warning_text()
            .unwrap()
            .starts_with(FEATURE));
    }
}
