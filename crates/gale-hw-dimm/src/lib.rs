pub mod backend;
#[cfg(windows)]
pub mod pawnio;
pub mod spd;

use gale_pawnio::PawnIoError;

pub const FEATURE: &str = "Memory temperature";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DimmStatus {
    Disabled,
    PawnIo(PawnIoError),
    NoModules,
    Io(String),
}

impl DimmStatus {
    pub fn warning_text(&self) -> Option<String> {
        match self {
            DimmStatus::Disabled | DimmStatus::NoModules => None,
            DimmStatus::PawnIo(error) => Some(error.warning_text(FEATURE)),
            DimmStatus::Io(message) => Some(format!("{FEATURE} is unavailable: {message}")),
        }
    }
}

#[cfg(windows)]
pub fn probe(enabled: bool) -> Result<backend::DimmBackend, DimmStatus> {
    if !enabled {
        return Err(DimmStatus::Disabled);
    }
    pawnio::probe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_driver_and_io_failures_produce_warnings() {
        assert_eq!(DimmStatus::Disabled.warning_text(), None);
        assert_eq!(DimmStatus::NoModules.warning_text(), None);
        assert!(DimmStatus::PawnIo(PawnIoError::NotInstalled)
            .warning_text()
            .unwrap()
            .starts_with(FEATURE));
    }
}
