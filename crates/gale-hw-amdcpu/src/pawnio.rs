use std::time::Duration;

use gale_pawnio::modules::AMDFAMILY17;
use gale_pawnio::mutex::PCI_BUS_MUTEX;
use gale_pawnio::{Module, NamedMutex, PawnIoError};

use crate::backend::{AmdCpuBackend, SmnReader};
use crate::AmdCpuStatus;

pub struct PawnIoSmn {
    module: Module,
    mutex: NamedMutex,
}

impl SmnReader for PawnIoSmn {
    fn read_smn(&mut self, address: u32) -> Result<u32, String> {
        let out = self
            .module
            .execute(c"ioctl_read_smn", &[address as u64], 1)?;
        out.first()
            .map(|v| *v as u32)
            .ok_or_else(|| "short read".to_string())
    }

    fn lock(&mut self, timeout: Duration) -> Result<bool, String> {
        self.mutex.lock(timeout)
    }

    fn unlock(&mut self) {
        self.mutex.unlock();
    }
}

struct CpuIdentity {
    model: u32,
    brand: String,
}

fn identify() -> Result<CpuIdentity, AmdCpuStatus> {
    let cpuid = raw_cpuid::CpuId::new();
    let vendor = cpuid
        .get_vendor_info()
        .map(|v| v.as_str().to_string())
        .unwrap_or_default();
    if vendor != "AuthenticAMD" {
        return Err(AmdCpuStatus::NotAmd);
    }
    let features = cpuid
        .get_feature_info()
        .ok_or(AmdCpuStatus::Unsupported("cpuid leaf 1 missing".into()))?;
    let family = u32::from(features.family_id());
    let model = u32::from(features.model_id());
    if !(0x17..=0x1A).contains(&family) {
        return Err(AmdCpuStatus::Unsupported(format!("family 0x{family:x}")));
    }
    let brand = cpuid
        .get_processor_brand_string()
        .map(|b| b.as_str().trim().to_string())
        .unwrap_or_default();
    Ok(CpuIdentity { model, brand })
}

pub fn probe() -> Result<AmdCpuBackend, AmdCpuStatus> {
    let identity = identify()?;
    let module = Module::load(&AMDFAMILY17).map_err(|error| match error {
        PawnIoError::ModuleLoad(_, hr) if hr as u32 == 0x8007_0032 => {
            AmdCpuStatus::Unsupported("module refused this cpu".into())
        }
        other => AmdCpuStatus::PawnIo(other),
    })?;
    let mutex = NamedMutex::open(PCI_BUS_MUTEX).map_err(AmdCpuStatus::Io)?;
    let mut backend = AmdCpuBackend::new(
        Box::new(PawnIoSmn { module, mutex }),
        identity.model,
        &identity.brand,
    );
    enumerate_with_retries(&mut backend).map_err(AmdCpuStatus::Io)?;
    Ok(backend)
}

const PROBE_ATTEMPTS: usize = 5;
const PROBE_RETRY_DELAY: Duration = Duration::from_millis(200);

fn enumerate_with_retries(backend: &mut dyn gale_hw::Backend) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 1..=PROBE_ATTEMPTS {
        match backend.enumerate() {
            Ok(_) => return Ok(()),
            Err(error) => {
                last = error.to_string();
                tracing::debug!(attempt, %last, "probe attempt failed");
                if attempt < PROBE_ATTEMPTS {
                    std::thread::sleep(PROBE_RETRY_DELAY);
                }
            }
        }
    }
    Err(last)
}
