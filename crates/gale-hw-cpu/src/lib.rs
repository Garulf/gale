pub mod backend;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod windows;

pub use backend::{CpuBackend, CpuFreq, CpuSources, CpuTimes, ProcStat, Rapl};

#[cfg(target_os = "linux")]
pub fn probe() -> CpuBackend {
    CpuBackend::new(linux::sources())
}

#[cfg(windows)]
pub fn probe() -> CpuBackend {
    CpuBackend::new(windows::sources())
}
