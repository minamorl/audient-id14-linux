//! Product definition table for the Audient iD14 mk1 and mk2.
//!
//! Every variant-specific value lives in [`PRODUCTS`]; nothing else in the
//! crate hard-codes a per-variant number.

use crate::error::ProtocolError;

/// USB vendor id shared by both iD14 variants.
pub const VID: u16 = 0x2708;
/// USB product id of the first-generation iD14.
pub const PID_MK1: u16 = 0x0002;
/// USB product id of the iD14 MKII.
pub const PID_MK2: u16 = 0x0008;

/// Hardware generation of the iD14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    /// First-generation iD14 (PID `0x0002`).
    Mk1,
    /// iD14 MKII (PID `0x0008`).
    Mk2,
}

/// One row of the product definition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProductDefinition {
    /// Hardware generation.
    pub variant: Variant,
    /// Human-readable product name.
    pub name: &'static str,
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
}

/// The product definition table. Both supported variants are listed here.
pub const PRODUCTS: [ProductDefinition; 2] = [
    ProductDefinition {
        variant: Variant::Mk1,
        name: "Audient iD14",
        vid: VID,
        pid: PID_MK1,
    },
    ProductDefinition {
        variant: Variant::Mk2,
        name: "Audient iD14 MKII",
        vid: VID,
        pid: PID_MK2,
    },
];

impl ProductDefinition {
    /// Look up the table row for a `(vid, pid)` pair.
    pub fn lookup(vid: u16, pid: u16) -> Result<ProductDefinition, ProtocolError> {
        PRODUCTS
            .iter()
            .copied()
            .find(|p| p.vid == vid && p.pid == pid)
            .ok_or(ProtocolError::UnknownProduct { vid, pid })
    }
}

impl Variant {
    /// The table row for this variant.
    pub fn definition(self) -> ProductDefinition {
        // Both variants are present in the table by construction.
        PRODUCTS
            .iter()
            .copied()
            .find(|p| p.variant == self)
            .unwrap_or(PRODUCTS[0])
    }
}

impl From<Variant> for ProductDefinition {
    fn from(v: Variant) -> Self {
        v.definition()
    }
}

impl TryFrom<(u16, u16)> for ProductDefinition {
    type Error = ProtocolError;

    fn try_from((vid, pid): (u16, u16)) -> Result<Self, Self::Error> {
        ProductDefinition::lookup(vid, pid)
    }
}

impl TryFrom<(u16, u16)> for Variant {
    type Error = ProtocolError;

    fn try_from(pair: (u16, u16)) -> Result<Self, Self::Error> {
        ProductDefinition::try_from(pair).map(|p| p.variant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_vid_pid() {
        assert_eq!(VID, 0x2708);
        assert_eq!(PID_MK1, 0x0002);
        assert_eq!(PID_MK2, 0x0008);
    }

    #[test]
    fn table_covers_both_variants() {
        assert_eq!(PRODUCTS.len(), 2);
        let mk1 = Variant::Mk1.definition();
        let mk2 = Variant::Mk2.definition();
        assert_eq!((mk1.vid, mk1.pid), (0x2708, 0x0002));
        assert_eq!((mk2.vid, mk2.pid), (0x2708, 0x0008));
    }

    #[test]
    fn lookup_by_vid_pid() {
        assert_eq!(Variant::try_from((0x2708, 0x0002)), Ok(Variant::Mk1));
        assert_eq!(Variant::try_from((0x2708, 0x0008)), Ok(Variant::Mk2));
        assert_eq!(
            Variant::try_from((0x2708, 0x0001)),
            Err(ProtocolError::UnknownProduct {
                vid: 0x2708,
                pid: 0x0001
            })
        );
        assert_eq!(
            Variant::try_from((0x1234, 0x0002)),
            Err(ProtocolError::UnknownProduct {
                vid: 0x1234,
                pid: 0x0002
            })
        );
    }
}
