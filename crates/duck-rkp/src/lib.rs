//! Remote Key Provisioning (RKP) workbench.
//!
//! Builds `AuthenticatedRequest` CSRs as specified by the AOSP IRemotelyProvisionedComponent
//! HAL (`hardware/interfaces/security/rkp`), talks to the RKP server and exports the returned
//! certificate chain as a Tricky Store style `keybox.xml`.

mod artifacts;
pub mod cbor;
pub mod certificates;
pub mod cli;
pub mod cose;
pub mod crypto_kdf;
pub mod error;
mod handlers;
pub mod http;
pub mod keybox_xml;
pub mod material;
pub mod patch_level;
pub mod private_output;
pub mod profile;
pub mod request;
pub mod validate;
pub mod verify;
mod workflow;

pub use cli::{Command, FEATURE, run};
