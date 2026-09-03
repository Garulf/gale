pub mod backend;
pub mod detect;
pub mod ffi_util;
pub mod module_blob;
pub mod nct677x;
#[cfg(windows)]
pub mod pawnio;
pub mod restore;
pub mod transport;

pub const PAWNIO_URL: &str = "https://pawnio.eu";

#[cfg(windows)]
pub fn probe() -> Result<backend::SuperIoBackend, SuperIoStatus> {
    let transport = pawnio::open()?;
    backend::SuperIoBackend::new(Box::new(transport))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuperIoStatus {
    PawnIoMissing,
    LibraryLoadFailed(String),
    PawnIoOpenFailed(i32),
    ModuleLoadFailed(i32),
    MutexTimeout,
    NoSupportedChip,
    Io(String),
}

impl SuperIoStatus {
    pub fn warning_text(&self) -> Option<String> {
        match self {
            SuperIoStatus::PawnIoMissing => Some(format!(
                "Motherboard fan control is unavailable: the PawnIO driver is not installed. Install it from {PAWNIO_URL} and restart the galed service."
            )),
            SuperIoStatus::LibraryLoadFailed(m) => Some(format!(
                "Motherboard fan control is unavailable: PawnIO is installed but PawnIOLib.dll could not be loaded ({m}). Reinstall from {PAWNIO_URL}."
            )),
            SuperIoStatus::PawnIoOpenFailed(hr) => Some(format!(
                "Motherboard fan control is unavailable: PawnIO is installed but could not be opened (HRESULT 0x{:08x}). See {PAWNIO_URL}.",
                *hr as u32
            )),
            SuperIoStatus::ModuleLoadFailed(hr) => Some(format!(
                "Motherboard fan control is unavailable: PawnIO rejected the LpcIO module (HRESULT 0x{:08x}). Update PawnIO from {PAWNIO_URL}.",
                *hr as u32
            )),
            SuperIoStatus::MutexTimeout => Some(
                "Motherboard fan control is unavailable: another program held the ISA bus lock for too long during startup. Restart the galed service."
                    .to_string(),
            ),
            SuperIoStatus::NoSupportedChip => None,
            SuperIoStatus::Io(m) => {
                Some(format!("Motherboard fan control is unavailable: {m}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pawnio_missing_warning_mentions_url() {
        let text = SuperIoStatus::PawnIoMissing.warning_text().unwrap();
        assert!(text.contains("https://pawnio.eu"));
        assert!(text.contains("driver is not installed"));
    }

    #[test]
    fn library_load_failed_warning_mentions_url_and_message() {
        let text = SuperIoStatus::LibraryLoadFailed("boom".to_string())
            .warning_text()
            .unwrap();
        assert!(text.contains("https://pawnio.eu"));
        assert!(text.contains("boom"));
    }

    #[test]
    fn pawnio_open_failed_renders_hresult() {
        let text = SuperIoStatus::PawnIoOpenFailed(-2147024891)
            .warning_text()
            .unwrap();
        assert!(text.contains("https://pawnio.eu"));
        assert!(text.contains("HRESULT 0x80070005"));
    }

    #[test]
    fn module_load_failed_renders_hresult() {
        let text = SuperIoStatus::ModuleLoadFailed(-2147024891)
            .warning_text()
            .unwrap();
        assert!(text.contains("https://pawnio.eu"));
        assert!(text.contains("HRESULT 0x80070005"));
    }

    #[test]
    fn mutex_timeout_warning_has_no_url() {
        let text = SuperIoStatus::MutexTimeout.warning_text().unwrap();
        assert!(text.contains("ISA bus lock"));
    }

    #[test]
    fn io_warning_wraps_message() {
        let text = SuperIoStatus::Io("disk full".to_string())
            .warning_text()
            .unwrap();
        assert_eq!(text, "Motherboard fan control is unavailable: disk full");
    }

    #[test]
    fn no_supported_chip_has_no_warning() {
        assert_eq!(SuperIoStatus::NoSupportedChip.warning_text(), None);
    }
}
