//! Explicit error representation for the recoverable fallible operations this
//! crate exposes. No operation in this crate panics on bad input.

use core::fmt;

/// Error returned by the fallible conversions in this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProtocolError {
    /// The `(vid, pid)` pair does not match any entry in the product
    /// definition table.
    UnknownProduct {
        /// USB vendor id that was looked up.
        vid: u16,
        /// USB product id that was looked up.
        pid: u16,
    },
    /// The value is not a known `getExtensionCode` return value.
    UnknownExtensionCode(u8),
}

impl ProtocolError {
    /// Stable machine-readable code for this error.
    pub const fn code(&self) -> &'static str {
        match self {
            ProtocolError::UnknownProduct { .. } => "unknown_product",
            ProtocolError::UnknownExtensionCode(_) => "unknown_extension_code",
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::UnknownProduct { vid, pid } => {
                write!(f, "unknown product {vid:04x}:{pid:04x}")
            }
            ProtocolError::UnknownExtensionCode(v) => {
                write!(f, "unknown extension code {v}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}
