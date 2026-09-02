pub const LPCIO_BIN: &[u8] = include_bytes!("../vendor/pawnio-modules/0.2.11/LpcIO.bin");
pub const LPCIO_BIN_SHA256: &str =
    "b3896a1cab0d808fca31fe2ebcae045d59dac690da87b17c858bb8da357eb45e";
pub const LPCIO_BIN_LEN: usize = 18076;
pub const PAWNIO_MODULES_VERSION: &str = "0.2.11";

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn lpcio_bin_has_expected_length() {
        assert_eq!(LPCIO_BIN.len(), LPCIO_BIN_LEN);
    }

    #[test]
    fn lpcio_bin_matches_expected_sha256() {
        let digest = Sha256::digest(LPCIO_BIN);
        assert_eq!(hex(&digest), LPCIO_BIN_SHA256);
    }
}
