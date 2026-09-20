//! `getExtensionCode` return values of the Audient Extension Unit.
//!
//! These are the values returned by that method in the official application.
//! They are **not** known to equal USB Unit IDs or command values sent to the
//! device; the crate therefore provides no conversion from [`ExtensionCode`]
//! to [`UsbUnitId`].

use crate::error::ProtocolError;

/// `getExtensionCode` return value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ExtensionCode {
    /// Routing matrix (`0`).
    RoutingMatrix = 0,
    /// Monitor controller (`1`).
    MonitorController = 1,
    /// Mono mix controller (`2`).
    MonoMixController = 2,
}

impl ExtensionCode {
    /// All known codes, in numeric order.
    pub const ALL: [ExtensionCode; 3] = [
        ExtensionCode::RoutingMatrix,
        ExtensionCode::MonitorController,
        ExtensionCode::MonoMixController,
    ];

    /// The numeric return value.
    pub const fn code(self) -> u8 {
        self as u8
    }
}

impl From<ExtensionCode> for u8 {
    fn from(c: ExtensionCode) -> Self {
        c.code()
    }
}

impl TryFrom<u8> for ExtensionCode {
    type Error = ProtocolError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(ExtensionCode::RoutingMatrix),
            1 => Ok(ExtensionCode::MonitorController),
            2 => Ok(ExtensionCode::MonoMixController),
            other => Err(ProtocolError::UnknownExtensionCode(other)),
        }
    }
}

/// A USB Audio Class unit id as it would appear on the wire.
///
/// This type is deliberately distinct from [`ExtensionCode`] and there is no
/// `From`/`TryFrom` between the two: the mapping is unknown, and a consumer
/// must not silently treat an extension code as a unit id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UsbUnitId(pub u8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_codes() {
        assert_eq!(ExtensionCode::RoutingMatrix.code(), 0);
        assert_eq!(ExtensionCode::MonitorController.code(), 1);
        assert_eq!(ExtensionCode::MonoMixController.code(), 2);
    }

    #[test]
    fn round_trip() {
        for c in ExtensionCode::ALL {
            assert_eq!(ExtensionCode::try_from(c.code()), Ok(c));
        }
        assert_eq!(
            ExtensionCode::try_from(3),
            Err(ProtocolError::UnknownExtensionCode(3))
        );
    }
}
