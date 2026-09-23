//! Per-row evidence status of the UAC2 control-request format.
//!
//! The crate-wide status ([`crate::PROTOCOL_EVIDENCE_STATUS`]) stays
//! "static analysis inferred, unverified": the protocol as a whole is not
//! claimed to be hardware confirmed. This module records, row by row, which
//! parts of the request format were observed on an iD14 MKII on 2026-09-24
//! (GET direction only) and which are still static inferences (SET).
//!
//! "Observed" is deliberately not called "confirmed": one direction was
//! observed on one unit, and nothing was checked on Linux.

use core::fmt;

/// Evidence status of one row of the request-format table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RowEvidence {
    /// Observed on an iD14 MKII (PID `0x0008`) on 2026-09-24.
    Mk2HardwareObserved20260924,
    /// Inferred from static analysis of the official app; unverified on
    /// hardware.
    StaticAnalysisInferredUnverified,
}

impl RowEvidence {
    /// Stable machine-readable label, identical to the value in the spec.
    pub const fn as_str(self) -> &'static str {
        match self {
            RowEvidence::Mk2HardwareObserved20260924 => "mk2_hardware_observed_2026-09-24",
            RowEvidence::StaticAnalysisInferredUnverified => "static_analysis_inferred_unverified",
        }
    }
}

impl fmt::Display for RowEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of the request-format table that carries its own evidence status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolRow {
    /// GET CUR header `a1 01`.
    GetHeader,
    /// GET RANGE header `a1 02`.
    GetRangeHeader,
    /// `wValue = (control selector << 8) | channel number`.
    WValueLayout,
    /// `wIndex = (entity id << 8) | control interface number`.
    WIndexLayout,
    /// SET CUR header `21 01`.
    SetHeader,
}

impl ProtocolRow {
    /// Every row, in table order.
    pub const ALL: [ProtocolRow; 5] = [
        ProtocolRow::GetHeader,
        ProtocolRow::GetRangeHeader,
        ProtocolRow::WValueLayout,
        ProtocolRow::WIndexLayout,
        ProtocolRow::SetHeader,
    ];

    /// Evidence status of this row.
    pub const fn evidence(self) -> RowEvidence {
        match self {
            ProtocolRow::GetHeader
            | ProtocolRow::GetRangeHeader
            | ProtocolRow::WValueLayout
            | ProtocolRow::WIndexLayout => RowEvidence::Mk2HardwareObserved20260924,
            ProtocolRow::SetHeader => RowEvidence::StaticAnalysisInferredUnverified,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_direction_rows_are_mk2_observed() {
        for row in [
            ProtocolRow::GetHeader,
            ProtocolRow::GetRangeHeader,
            ProtocolRow::WValueLayout,
            ProtocolRow::WIndexLayout,
        ] {
            assert_eq!(row.evidence(), RowEvidence::Mk2HardwareObserved20260924);
            assert_eq!(row.evidence().as_str(), "mk2_hardware_observed_2026-09-24");
        }
    }

    #[test]
    fn set_header_stays_static_inferred() {
        assert_eq!(
            ProtocolRow::SetHeader.evidence(),
            RowEvidence::StaticAnalysisInferredUnverified
        );
        assert_eq!(
            ProtocolRow::SetHeader.evidence().to_string(),
            "static_analysis_inferred_unverified"
        );
    }
}
