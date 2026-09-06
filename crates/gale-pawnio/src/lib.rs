pub mod error;
pub mod modules;
pub mod mutex;

#[cfg(windows)]
pub mod driver;

#[cfg(windows)]
pub use driver::{is_installed, Module};
pub use error::{PawnIoError, PAWNIO_URL};
#[cfg(windows)]
pub use mutex::{MutexGuard, NamedMutex};
