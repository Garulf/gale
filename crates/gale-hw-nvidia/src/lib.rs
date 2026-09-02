pub mod backend;
pub mod facade;

pub use backend::NvidiaBackend;
pub use facade::{NvmlDevice, NvmlFacade, RealNvml};
