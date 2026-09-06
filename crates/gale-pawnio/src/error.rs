pub const PAWNIO_URL: &str = "https://pawnio.eu";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PawnIoError {
    NotInstalled,
    LibraryLoad(String),
    Open(i32),
    ModuleLoad(&'static str, i32),
}

impl PawnIoError {
    pub fn warning_text(&self, feature: &str) -> String {
        match self {
            PawnIoError::NotInstalled => format!(
                "{feature} is unavailable: the PawnIO driver is not installed. Install it from {PAWNIO_URL} and restart the galed service."
            ),
            PawnIoError::LibraryLoad(message) => format!(
                "{feature} is unavailable: PawnIO is installed but PawnIOLib.dll could not be loaded ({message}). Reinstall from {PAWNIO_URL}."
            ),
            PawnIoError::Open(hr) => format!(
                "{feature} is unavailable: PawnIO is installed but could not be opened (HRESULT 0x{:08x}). See {PAWNIO_URL}.",
                *hr as u32
            ),
            PawnIoError::ModuleLoad(module, hr) => format!(
                "{feature} is unavailable: PawnIO rejected the {module} module (HRESULT 0x{:08x}). Update PawnIO from {PAWNIO_URL}.",
                *hr as u32
            ),
        }
    }
}

pub fn hresult_failed(hr: i32) -> bool {
    hr < 0
}

pub fn hresult_message(call: &str, hr: i32) -> String {
    format!("{call} failed with HRESULT 0x{:08x}", hr as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_texts_name_the_feature_and_the_url() {
        for error in [
            PawnIoError::NotInstalled,
            PawnIoError::LibraryLoad("boom".to_string()),
            PawnIoError::Open(-1),
            PawnIoError::ModuleLoad("LpcIO", -2),
        ] {
            let text = error.warning_text("Motherboard fan control");
            assert!(
                text.starts_with("Motherboard fan control is unavailable"),
                "{text}"
            );
            assert!(text.contains(PAWNIO_URL), "{text}");
        }
        assert!(PawnIoError::LibraryLoad("boom".to_string())
            .warning_text("x")
            .contains("boom"));
        assert!(PawnIoError::ModuleLoad("LpcIO", -2)
            .warning_text("x")
            .contains("LpcIO"));
    }

    #[test]
    fn hresult_helpers() {
        assert!(hresult_failed(-1));
        assert!(!hresult_failed(0));
        assert_eq!(hresult_message("f", -1), "f failed with HRESULT 0xffffffff");
    }
}
