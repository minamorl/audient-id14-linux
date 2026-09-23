//! The control-transfer capability the operations are written against.
//!
//! The live implementation is [`crate::usb::RusbTransport`]; tests inject a
//! scripted fake. `--dry-run` never constructs a transport at all, so it
//! cannot perform a transfer.

use std::error::Error;
use std::fmt;

use id14_protocol::SetupPacket;

use crate::ops::format_bytes;

/// Minimal capability: send one class control request.
pub trait ControlTransport {
    /// Send a device-to-host request and return the bytes received.
    fn control_in(&mut self, setup: &SetupPacket) -> Result<Vec<u8>, TransportError>;

    /// Send a host-to-device request carrying `payload`.
    fn control_out(&mut self, setup: &SetupPacket, payload: &[u8]) -> Result<(), TransportError>;
}

/// A failed control transfer, with the request that failed and its cause.
#[derive(Debug)]
pub struct TransportError {
    /// The request that failed.
    pub setup: SetupPacket,
    /// Underlying cause.
    pub cause: Box<dyn Error + Send + Sync>,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] failed: {}",
            self.setup.attribute.label(),
            format_bytes(&self.setup.to_bytes()),
            self.cause
        )
    }
}

impl Error for TransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
