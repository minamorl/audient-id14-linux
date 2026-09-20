//! Pure, host-independent iD14 control-protocol library (packet construction
//! and interpretation only; no USB access).
//!
//! Every numeric value in this crate (VID/PID, request headers, extension
//! codes) is a **static-analysis inference** transcribed from the research
//! notes and has **not** been verified against hardware. See
//! [`evidence::PROTOCOL_EVIDENCE_STATUS`].
//!
//! This crate has no dependencies and performs no I/O, so its unit tests run
//! with no device and no host-specific support.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod evidence;
pub mod extension;
pub mod product;
pub mod request;

pub use error::ProtocolError;
pub use evidence::{EvidenceStatus, PROTOCOL_EVIDENCE_STATUS};
pub use extension::{ExtensionCode, UsbUnitId};
pub use product::{ProductDefinition, Variant, PID_MK1, PID_MK2, PRODUCTS, VID};
pub use request::{ControlRequest, RequestKind};
