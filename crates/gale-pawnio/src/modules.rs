pub const PAWNIO_MODULES_VERSION: &str = "0.2.11";

pub struct ModuleBlob {
    pub name: &'static str,
    pub bytes: &'static [u8],
    pub sha256: &'static str,
    pub len: usize,
}

pub const LPCIO: ModuleBlob = ModuleBlob {
    name: "LpcIO",
    bytes: include_bytes!("../vendor/pawnio-modules/0.2.11/LpcIO.bin"),
    sha256: "b3896a1cab0d808fca31fe2ebcae045d59dac690da87b17c858bb8da357eb45e",
    len: 18076,
};

pub const AMDFAMILY17: ModuleBlob = ModuleBlob {
    name: "AMDFamily17",
    bytes: include_bytes!("../vendor/pawnio-modules/0.2.11/AMDFamily17.bin"),
    sha256: "dae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91",
    len: 10652,
};

pub const SMBUSPIIX4: ModuleBlob = ModuleBlob {
    name: "SmbusPIIX4",
    bytes: include_bytes!("../vendor/pawnio-modules/0.2.11/SmbusPIIX4.bin"),
    sha256: "91f9b4b1c39e3d399ce48477a89d8f6bd3e58a2241064a4daec3a1513dff56e5",
    len: 42404,
};

pub const LPCACPIEC: ModuleBlob = ModuleBlob {
    name: "LpcACPIEC",
    bytes: include_bytes!("../vendor/pawnio-modules/0.2.11/LpcACPIEC.bin"),
    sha256: "c38fd116e7aff4d1fdb0a494e296be0a6708e5a22fc72f14587442fb7f8f7906",
    len: 2612,
};

pub const ALL: [&ModuleBlob; 4] = [&LPCIO, &AMDFAMILY17, &SMBUSPIIX4, &LPCACPIEC];

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn every_vendored_module_matches_its_recorded_length_and_sha256() {
        for module in ALL {
            assert_eq!(module.bytes.len(), module.len, "{}", module.name);
            assert_eq!(
                hex(&Sha256::digest(module.bytes)),
                module.sha256,
                "{}",
                module.name
            );
        }
    }
}
