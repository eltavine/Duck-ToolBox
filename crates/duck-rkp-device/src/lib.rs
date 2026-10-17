//! Device-backed RKP provisioning. Vendor ABI access, TA framing, provisioning and
//! artifact ownership stay inside this feature; RKP protocol primitives come from duck-rkp.

mod artifacts;
mod cli;
mod error;
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
#[allow(unsafe_code)]
mod keymint;
mod protocol;
mod service;
mod soc;
mod status;
mod worker;

pub use cli::FEATURE;
