//! Pure, host-independent iD14 control-protocol library (packet construction
//! and interpretation only; no USB access).
//!
//! Every numeric value in this crate (VID/PID, request headers, extension
//! codes) is a **static-analysis inference** transcribed from the research
//! notes; the protocol as a whole is **not** claimed to be hardware confirmed
//! (see [`evidence::PROTOCOL_EVIDENCE_STATUS`]). Individual rows of the UAC2
//! request format carry their own status in [`row_evidence`]: the GET / GET
//! RANGE headers and the `wValue` / `wIndex` layouts were observed on an iD14
//! MKII on 2026-09-24; SET is still a static inference.
//!
//! The control path ([`uac2`], [`descriptor`], [`plan`]) is a UAC2 class
//! request addressed to the DFU interface found in the descriptor; entity ids
//! are read from the descriptor at run time.
//!
//! This crate has no dependencies and performs no I/O, so its unit tests run
//! with no device and no host-specific support.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod control_error;
pub mod db;
pub mod descriptor;
pub mod error;
pub mod evidence;
pub mod extension;
pub mod fixtures;
pub mod plan;
pub mod product;
pub mod request;
pub mod row_evidence;
pub mod uac2;

pub use error::ProtocolError;
pub use evidence::{EvidenceStatus, PROTOCOL_EVIDENCE_STATUS};
pub use extension::{ExtensionCode, UsbUnitId};
pub use product::{ProductDefinition, Variant, PID_MK1, PID_MK2, PRODUCTS, VID};
pub use request::{ControlRequest, RequestKind};

pub use control_error::ControlError;
pub use descriptor::{AudioControl, ControlInterface, InterfaceInfo};
pub use row_evidence::{ProtocolRow, RowEvidence};
pub use uac2::{ControlAddress, RequestAttribute, SetupPacket};
