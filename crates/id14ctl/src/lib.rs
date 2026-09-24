//! Library target for the `id14ctl` package.
//!
//! Control requests are UAC2 class requests addressed to the DFU interface
//! found in the descriptor (the only interface this tool claims). Entity ids
//! come from the descriptor at run time. The operations in [`ops`] are
//! written against the [`transport::ControlTransport`] capability; the live
//! implementation is [`usb::RusbTransport`], and `--dry-run` uses no
//! transport at all.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod ops;
pub mod transport;
pub mod usb;

pub use error::CliError;
