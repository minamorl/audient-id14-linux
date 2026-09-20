//! Evidence status of the protocol values carried by this crate.
//!
//! The values were inferred by static analysis of the official application
//! and have not been checked against real hardware. This module makes that
//! status explicit and makes a "hardware confirmed" status unrepresentable.

use core::fmt;

/// Confirmation status of a protocol value.
///
/// There is intentionally **no** `HardwareConfirmed` variant: nothing in this
/// crate may claim hardware confirmation. Adding such a variant is a human
/// decision that requires a real device log, not a code change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EvidenceStatus {
    /// Inferred from static analysis of the official app; unverified on
    /// hardware.
    StaticAnalysisInferredUnverified,
}

impl EvidenceStatus {
    /// Always `false`: no value in this crate is hardware-confirmed.
    pub const fn is_hardware_confirmed(self) -> bool {
        false
    }

    /// Stable string form of the status.
    pub const fn as_str(self) -> &'static str {
        match self {
            EvidenceStatus::StaticAnalysisInferredUnverified => {
                "static_analysis_inferred_unverified"
            }
        }
    }
}

impl fmt::Display for EvidenceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Evidence status applying to every constant in this crate.
pub const PROTOCOL_EVIDENCE_STATUS: EvidenceStatus =
    EvidenceStatus::StaticAnalysisInferredUnverified;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_static_inferred_unverified() {
        assert_eq!(
            PROTOCOL_EVIDENCE_STATUS,
            EvidenceStatus::StaticAnalysisInferredUnverified
        );
        assert_eq!(
            PROTOCOL_EVIDENCE_STATUS.as_str(),
            "static_analysis_inferred_unverified"
        );
    }

    #[test]
    fn never_hardware_confirmed() {
        assert!(!PROTOCOL_EVIDENCE_STATUS.is_hardware_confirmed());
    }
}
