//! Explicit CLI error representation. Every variant keeps its cause.

use std::error::Error;
use std::fmt;
use std::io;

use id14_protocol::{ControlError, ProtocolError};

use crate::transport::TransportError;

/// Error returned by the `id14ctl` operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum CliError {
    /// A libusb call failed.
    Usb {
        /// What was being done.
        context: &'static str,
        /// libusb error.
        source: rusb::Error,
    },
    /// Descriptor interpretation or request planning failed.
    Control(ControlError),
    /// Product lookup failed.
    Protocol(ProtocolError),
    /// A control transfer failed.
    Transport(TransportError),
    /// Writing the output failed.
    Output(io::Error),
    /// No supported device is connected.
    NoDevice,
    /// A write operation was requested without `--enable-write`.
    WriteDisabled,
    /// The device declares no mute control.
    NoDeclaredMuteControl,
    /// The device declares mute controls, but writing them is not decided by
    /// the specification.
    MuteNotSpecified(Vec<(u8, u8)>),
    /// `request` only prints bytes; it never sends.
    RequestSendUnsupported,
    /// Some dump reads failed (each failure was printed in place).
    DumpIncomplete {
        /// Number of failed reads.
        failed: usize,
    },
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usb { context, source } => write!(f, "{context}: {source}"),
            CliError::Control(e) => write!(f, "{e}"),
            CliError::Protocol(e) => write!(f, "{e}"),
            CliError::Transport(e) => write!(f, "{e}"),
            CliError::Output(e) => write!(f, "cannot write output: {e}"),
            CliError::NoDevice => f.write_str("no supported Audient iD14 device found"),
            CliError::WriteDisabled => {
                f.write_str("write operations are disabled; pass --enable-write to allow them")
            }
            CliError::NoDeclaredMuteControl => f.write_str(
                "the device declares no mute control; mute is not available (no substitute is used)",
            ),
            CliError::MuteNotSpecified(declared) => write!(
                f,
                "the device declares mute controls {declared:02x?}, but writing them is not specified yet"
            ),
            CliError::RequestSendUnsupported => f.write_str(
                "`request` only prints the bytes it would send; run it with --dry-run",
            ),
            CliError::DumpIncomplete { failed } => {
                write!(f, "{failed} read(s) failed during dump")
            }
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CliError::Usb { source, .. } => Some(source),
            CliError::Control(e) => Some(e),
            CliError::Transport(e) => Some(e),
            CliError::Output(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ControlError> for CliError {
    fn from(value: ControlError) -> Self {
        CliError::Control(value)
    }
}

impl From<ProtocolError> for CliError {
    fn from(value: ProtocolError) -> Self {
        CliError::Protocol(value)
    }
}

impl From<TransportError> for CliError {
    fn from(value: TransportError) -> Self {
        CliError::Transport(value)
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        CliError::Output(value)
    }
}
