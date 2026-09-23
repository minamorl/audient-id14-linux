//! USB Audio Class 2.0 control requests: setup-packet construction and
//! parameter-block decoding (UAC2 §5.2.2-5.2.3).
//!
//! The control path is a class request addressed to an interface: `wIndex`
//! carries `(entity id << 8) | control interface number`, and the control
//! interface is the DFU interface found in the descriptor
//! ([`crate::descriptor::find_control_interface`]).

use crate::control_error::ControlError;
use crate::row_evidence::{ProtocolRow, RowEvidence};

/// `bmRequestType` of a class request to an interface, device-to-host.
pub const REQUEST_TYPE_CLASS_INTERFACE_IN: u8 = 0xA1;
/// `bmRequestType` of a class request to an interface, host-to-device.
pub const REQUEST_TYPE_CLASS_INTERFACE_OUT: u8 = 0x21;
/// `bRequest` for the CUR attribute.
pub const REQUEST_CUR: u8 = 0x01;
/// `bRequest` for the RANGE attribute.
pub const REQUEST_RANGE: u8 = 0x02;

/// Clock Source control selector: sampling frequency.
pub const CS_SAM_FREQ_CONTROL: u8 = 0x01;
/// Clock Selector control selector.
pub const CX_CLOCK_SELECTOR_CONTROL: u8 = 0x01;
/// Mixer Unit control selector: mixer control (low byte of `wValue` is the
/// Mixer Control Number).
pub const MU_MIXER_CONTROL: u8 = 0x01;

/// Which attribute a request reads or writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestAttribute {
    /// GET CUR (`a1 01`).
    GetCur,
    /// GET RANGE (`a1 02`).
    GetRange,
    /// SET CUR (`21 01`).
    SetCur,
}

impl RequestAttribute {
    /// `bmRequestType` of the request.
    pub const fn bm_request_type(self) -> u8 {
        match self {
            RequestAttribute::GetCur | RequestAttribute::GetRange => {
                REQUEST_TYPE_CLASS_INTERFACE_IN
            }
            RequestAttribute::SetCur => REQUEST_TYPE_CLASS_INTERFACE_OUT,
        }
    }

    /// `bRequest` of the request.
    pub const fn b_request(self) -> u8 {
        match self {
            RequestAttribute::GetCur | RequestAttribute::SetCur => REQUEST_CUR,
            RequestAttribute::GetRange => REQUEST_RANGE,
        }
    }

    /// Whether the request moves data from the device to the host.
    pub const fn is_read(self) -> bool {
        !matches!(self, RequestAttribute::SetCur)
    }

    /// Evidence status of this request's header row.
    pub const fn header_evidence(self) -> RowEvidence {
        match self {
            RequestAttribute::GetCur => ProtocolRow::GetHeader.evidence(),
            RequestAttribute::GetRange => ProtocolRow::GetRangeHeader.evidence(),
            RequestAttribute::SetCur => ProtocolRow::SetHeader.evidence(),
        }
    }

    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            RequestAttribute::GetCur => "GET CUR",
            RequestAttribute::GetRange => "GET RANGE",
            RequestAttribute::SetCur => "SET CUR",
        }
    }
}

/// Address of one control: entity, selector, channel, and control interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlAddress {
    /// Entity id (unit, terminal or clock entity) read from the descriptor.
    pub entity_id: u8,
    /// Control selector.
    pub control_selector: u8,
    /// Channel number, or Mixer Control Number for mixer controls.
    pub channel: u8,
    /// Interface number placed in the low byte of `wIndex` (the DFU interface).
    pub interface_number: u8,
}

impl ControlAddress {
    /// `wValue = (control selector << 8) | channel number`.
    pub const fn w_value(&self) -> u16 {
        ((self.control_selector as u16) << 8) | self.channel as u16
    }

    /// `wIndex = (entity id << 8) | control interface number`.
    pub const fn w_index(&self) -> u16 {
        ((self.entity_id as u16) << 8) | self.interface_number as u16
    }
}

/// An 8-byte control setup packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetupPacket {
    /// Attribute requested.
    pub attribute: RequestAttribute,
    /// Addressed control.
    pub address: ControlAddress,
    /// `wLength`.
    pub w_length: u16,
}

impl SetupPacket {
    /// GET CUR of `w_length` bytes.
    pub const fn get_cur(address: ControlAddress, w_length: u16) -> Self {
        SetupPacket {
            attribute: RequestAttribute::GetCur,
            address,
            w_length,
        }
    }

    /// GET RANGE of `w_length` bytes.
    pub const fn get_range(address: ControlAddress, w_length: u16) -> Self {
        SetupPacket {
            attribute: RequestAttribute::GetRange,
            address,
            w_length,
        }
    }

    /// SET CUR carrying `w_length` payload bytes.
    pub const fn set_cur(address: ControlAddress, w_length: u16) -> Self {
        SetupPacket {
            attribute: RequestAttribute::SetCur,
            address,
            w_length,
        }
    }

    /// `bmRequestType`.
    pub const fn bm_request_type(&self) -> u8 {
        self.attribute.bm_request_type()
    }

    /// `bRequest`.
    pub const fn b_request(&self) -> u8 {
        self.attribute.b_request()
    }

    /// `wValue`.
    pub const fn w_value(&self) -> u16 {
        self.address.w_value()
    }

    /// `wIndex`.
    pub const fn w_index(&self) -> u16 {
        self.address.w_index()
    }

    /// The 8 bytes on the wire (multi-byte fields little-endian).
    pub const fn to_bytes(&self) -> [u8; 8] {
        let v = self.w_value().to_le_bytes();
        let i = self.w_index().to_le_bytes();
        let l = self.w_length.to_le_bytes();
        [
            self.bm_request_type(),
            self.b_request(),
            v[0],
            v[1],
            i[0],
            i[1],
            l[0],
            l[1],
        ]
    }
}

impl From<SetupPacket> for [u8; 8] {
    fn from(packet: SetupPacket) -> Self {
        packet.to_bytes()
    }
}

/// Size of a parameter block's CUR/MIN/MAX/RES fields (UAC2 layouts 1-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamLayout {
    /// Layout 1: 1-byte fields.
    One,
    /// Layout 2: 2-byte fields.
    Two,
    /// Layout 3: 4-byte fields.
    Four,
}

impl ParamLayout {
    /// Field size in bytes.
    pub const fn size(self) -> usize {
        match self {
            ParamLayout::One => 1,
            ParamLayout::Two => 2,
            ParamLayout::Four => 4,
        }
    }

    /// `wLength` of a CUR parameter block.
    pub const fn cur_length(self) -> u16 {
        self.size() as u16
    }

    /// `wLength` of the first RANGE read, which fetches `wNumSubRanges` only.
    pub const fn range_count_length() -> u16 {
        2
    }

    /// `wLength` of a full RANGE parameter block with `subranges` entries.
    pub fn range_length(self, subranges: u16) -> Result<u16, ControlError> {
        let total = 2usize + 3 * self.size() * subranges as usize;
        u16::try_from(total).map_err(|_| ControlError::LengthOverflow)
    }

    fn read_raw(self, bytes: &[u8]) -> u32 {
        match self {
            ParamLayout::One => bytes[0] as u32,
            ParamLayout::Two => u16::from_le_bytes([bytes[0], bytes[1]]) as u32,
            ParamLayout::Four => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        }
    }

    /// Sign-extend a raw field of this size.
    pub const fn signed(self, raw: u32) -> i64 {
        match self {
            ParamLayout::One => raw as u8 as i8 as i64,
            ParamLayout::Two => raw as u16 as i16 as i64,
            ParamLayout::Four => raw as i32 as i64,
        }
    }
}

/// Decode a CUR parameter block into its raw (unsigned) field value. Bytes
/// beyond the field are ignored.
pub fn parse_cur(layout: ParamLayout, bytes: &[u8]) -> Result<u32, ControlError> {
    if bytes.len() < layout.size() {
        return Err(ControlError::ShortResponse {
            expected: layout.size(),
            actual: bytes.len(),
        });
    }
    Ok(layout.read_raw(bytes))
}

/// Encode a CUR parameter block for SET CUR.
pub fn encode_cur(layout: ParamLayout, raw: u32) -> Vec<u8> {
    let le = raw.to_le_bytes();
    le[..layout.size()].to_vec()
}

/// Decode `wNumSubRanges` from the first two bytes of a RANGE block.
pub fn parse_range_count(bytes: &[u8]) -> Result<u16, ControlError> {
    if bytes.len() < 2 {
        return Err(ControlError::ShortResponse {
            expected: 2,
            actual: bytes.len(),
        });
    }
    match u16::from_le_bytes([bytes[0], bytes[1]]) {
        0 => Err(ControlError::EmptyRange),
        n => Ok(n),
    }
}

/// One `(MIN, MAX, RES)` subrange, raw (unsigned) field values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subrange {
    /// MIN.
    pub min: u32,
    /// MAX.
    pub max: u32,
    /// RES.
    pub res: u32,
}

/// A decoded RANGE parameter block.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RangeBlock {
    /// Field size of the block.
    pub layout: ParamLayout,
    /// Subranges in device order.
    pub subranges: Vec<Subrange>,
}

impl RangeBlock {
    /// `(min, max)` of each subrange, sign-extended.
    pub fn signed_bounds(&self) -> Vec<(i64, i64)> {
        self.subranges
            .iter()
            .map(|s| (self.layout.signed(s.min), self.layout.signed(s.max)))
            .collect()
    }

    /// Reject `value` (sign-extended raw unit) unless it lies inside some
    /// subrange.
    pub fn check_signed(&self, value: i64) -> Result<(), ControlError> {
        let bounds = self.signed_bounds();
        if bounds.iter().any(|&(lo, hi)| lo <= value && value <= hi) {
            Ok(())
        } else {
            Err(ControlError::OutOfRange {
                requested: value,
                ranges: bounds,
            })
        }
    }
}

/// Decode a full RANGE parameter block. Bytes beyond the declared subranges
/// are ignored (the device may pad a response with stale buffer contents).
pub fn parse_range(layout: ParamLayout, bytes: &[u8]) -> Result<RangeBlock, ControlError> {
    let count = parse_range_count(bytes)? as usize;
    let size = layout.size();
    let expected = 2 + 3 * size * count;
    if bytes.len() < expected {
        return Err(ControlError::ShortResponse {
            expected,
            actual: bytes.len(),
        });
    }
    let subranges = (0..count)
        .map(|i| {
            let base = 2 + 3 * size * i;
            Subrange {
                min: layout.read_raw(&bytes[base..]),
                max: layout.read_raw(&bytes[base + size..]),
                res: layout.read_raw(&bytes[base + 2 * size..]),
            }
        })
        .collect();
    Ok(RangeBlock { layout, subranges })
}

/// Feature Unit controls (UAC2 A.17.7), in `bmaControls` bit-pair order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureControl {
    /// Mute (selector 0x01).
    Mute,
    /// Volume (0x02).
    Volume,
    /// Bass (0x03).
    Bass,
    /// Mid (0x04).
    Mid,
    /// Treble (0x05).
    Treble,
    /// Graphic equalizer (0x06).
    GraphicEqualizer,
    /// Automatic gain (0x07).
    AutomaticGain,
    /// Delay (0x08).
    Delay,
    /// Bass boost (0x09).
    BassBoost,
    /// Loudness (0x0A).
    Loudness,
    /// Input gain (0x0B).
    InputGain,
    /// Input gain pad (0x0C).
    InputGainPad,
    /// Phase inverter (0x0D).
    PhaseInverter,
    /// Underflow (0x0E).
    Underflow,
    /// Overflow (0x0F).
    Overflow,
}

/// How a control's CUR value is interpreted for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueKind {
    /// TRUE/FALSE.
    Bool,
    /// Signed 1/256 dB (0x8000 = -inf).
    Decibel256,
    /// Signed 1/4 dB.
    DecibelQuarter,
    /// Unsigned integer.
    Unsigned,
    /// Sampling frequency in Hz.
    Hertz,
    /// One-based clock selector input pin.
    SelectorPin,
    /// Uninterpreted bytes.
    Raw,
}

impl FeatureControl {
    /// Every Feature Unit control, in selector order.
    pub const ALL: [FeatureControl; 15] = [
        FeatureControl::Mute,
        FeatureControl::Volume,
        FeatureControl::Bass,
        FeatureControl::Mid,
        FeatureControl::Treble,
        FeatureControl::GraphicEqualizer,
        FeatureControl::AutomaticGain,
        FeatureControl::Delay,
        FeatureControl::BassBoost,
        FeatureControl::Loudness,
        FeatureControl::InputGain,
        FeatureControl::InputGainPad,
        FeatureControl::PhaseInverter,
        FeatureControl::Underflow,
        FeatureControl::Overflow,
    ];

    /// Control selector.
    pub const fn selector(self) -> u8 {
        match self {
            FeatureControl::Mute => 0x01,
            FeatureControl::Volume => 0x02,
            FeatureControl::Bass => 0x03,
            FeatureControl::Mid => 0x04,
            FeatureControl::Treble => 0x05,
            FeatureControl::GraphicEqualizer => 0x06,
            FeatureControl::AutomaticGain => 0x07,
            FeatureControl::Delay => 0x08,
            FeatureControl::BassBoost => 0x09,
            FeatureControl::Loudness => 0x0A,
            FeatureControl::InputGain => 0x0B,
            FeatureControl::InputGainPad => 0x0C,
            FeatureControl::PhaseInverter => 0x0D,
            FeatureControl::Underflow => 0x0E,
            FeatureControl::Overflow => 0x0F,
        }
    }

    /// Short name.
    pub const fn name(self) -> &'static str {
        match self {
            FeatureControl::Mute => "MUTE",
            FeatureControl::Volume => "VOLUME",
            FeatureControl::Bass => "BASS",
            FeatureControl::Mid => "MID",
            FeatureControl::Treble => "TREBLE",
            FeatureControl::GraphicEqualizer => "GRAPHIC_EQ",
            FeatureControl::AutomaticGain => "AGC",
            FeatureControl::Delay => "DELAY",
            FeatureControl::BassBoost => "BASS_BOOST",
            FeatureControl::Loudness => "LOUDNESS",
            FeatureControl::InputGain => "INPUT_GAIN",
            FeatureControl::InputGainPad => "INPUT_GAIN_PAD",
            FeatureControl::PhaseInverter => "PHASE_INVERTER",
            FeatureControl::Underflow => "UNDERFLOW",
            FeatureControl::Overflow => "OVERFLOW",
        }
    }

    /// Parameter layout of the CUR block, or `None` when its length is
    /// variable (graphic equalizer).
    pub const fn layout(self) -> Option<ParamLayout> {
        match self {
            FeatureControl::Volume | FeatureControl::InputGain | FeatureControl::InputGainPad => {
                Some(ParamLayout::Two)
            }
            FeatureControl::Delay => Some(ParamLayout::Four),
            FeatureControl::GraphicEqualizer => None,
            _ => Some(ParamLayout::One),
        }
    }

    /// `wLength` used to read the CUR block. The graphic equalizer block is
    /// `4 + NrBits` bytes; the maximum (32 bands) is requested.
    pub const fn cur_length(self) -> u16 {
        match self.layout() {
            Some(layout) => layout.cur_length(),
            None => 4 + 32,
        }
    }

    /// Interpretation of the CUR value.
    pub const fn value_kind(self) -> ValueKind {
        match self {
            FeatureControl::Volume | FeatureControl::InputGain | FeatureControl::InputGainPad => {
                ValueKind::Decibel256
            }
            FeatureControl::Bass | FeatureControl::Mid | FeatureControl::Treble => {
                ValueKind::DecibelQuarter
            }
            FeatureControl::Delay => ValueKind::Unsigned,
            FeatureControl::GraphicEqualizer => ValueKind::Raw,
            _ => ValueKind::Bool,
        }
    }
}

impl TryFrom<u8> for FeatureControl {
    type Error = u8;

    fn try_from(selector: u8) -> Result<Self, Self::Error> {
        FeatureControl::ALL
            .into_iter()
            .find(|c| c.selector() == selector)
            .ok_or(selector)
    }
}

/// Capability of one control as declared by a bit pair in `bmControls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlCapability {
    /// `0b00`: not present.
    Absent,
    /// `0b01`: present, read-only.
    ReadOnly,
    /// `0b11`: present, host programmable.
    HostProgrammable,
    /// `0b10`: not allowed by the specification.
    Invalid,
}

impl ControlCapability {
    /// Decode the bit pair at `pair_index` (0-based) of `bitmap`.
    pub const fn from_bitmap(bitmap: u32, pair_index: u32) -> Self {
        match (bitmap >> (pair_index * 2)) & 0b11 {
            0b00 => ControlCapability::Absent,
            0b01 => ControlCapability::ReadOnly,
            0b11 => ControlCapability::HostProgrammable,
            _ => ControlCapability::Invalid,
        }
    }

    /// Whether the control is declared (present and readable).
    pub const fn is_declared(self) -> bool {
        matches!(
            self,
            ControlCapability::ReadOnly | ControlCapability::HostProgrammable
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fu_volume(channel: u8) -> ControlAddress {
        ControlAddress {
            entity_id: 0x0C,
            control_selector: FeatureControl::Volume.selector(),
            channel,
            interface_number: 4,
        }
    }

    #[test]
    fn get_cur_setup_packet_bytes() {
        let packet = SetupPacket::get_cur(fu_volume(1), 2);
        assert_eq!(
            packet.to_bytes(),
            [0xa1, 0x01, 0x01, 0x02, 0x04, 0x0c, 0x02, 0x00]
        );
        assert_eq!(packet.w_value(), 0x0201);
        assert_eq!(packet.w_index(), 0x0c04);
    }

    #[test]
    fn get_range_and_set_cur_headers() {
        let range = SetupPacket::get_range(fu_volume(2), 8).to_bytes();
        assert_eq!(&range[..2], &[0xa1, 0x02]);
        let set: [u8; 8] = SetupPacket::set_cur(fu_volume(2), 2).into();
        assert_eq!(set, [0x21, 0x01, 0x02, 0x02, 0x04, 0x0c, 0x02, 0x00]);
    }

    #[test]
    fn setup_headers_match_the_pinned_request_headers() {
        use crate::request::RequestKind;
        let cur = SetupPacket::get_cur(fu_volume(1), 2).to_bytes();
        let set = SetupPacket::set_cur(fu_volume(1), 2).to_bytes();
        assert_eq!([cur[0], cur[1]], RequestKind::Get.header_bytes());
        assert_eq!([set[0], set[1]], RequestKind::Set.header_bytes());
        assert_eq!(u16::from_le_bytes([cur[0], cur[1]]), 0x01a1);
        assert_eq!(u16::from_le_bytes([set[0], set[1]]), 0x0121);
    }

    #[test]
    fn header_evidence_follows_rows() {
        assert_eq!(
            RequestAttribute::GetCur.header_evidence(),
            RowEvidence::Mk2HardwareObserved20260924
        );
        assert_eq!(
            RequestAttribute::GetRange.header_evidence(),
            RowEvidence::Mk2HardwareObserved20260924
        );
        assert_eq!(
            RequestAttribute::SetCur.header_evidence(),
            RowEvidence::StaticAnalysisInferredUnverified
        );
    }

    #[test]
    fn range_block_decoding_ignores_trailing_bytes() {
        // one subrange: -127 dB .. 0 dB step 1 dB, then stale bytes
        let mut bytes = vec![0x01, 0x00];
        bytes.extend_from_slice(&(-127i16 * 256).to_le_bytes());
        bytes.extend_from_slice(&0i16.to_le_bytes());
        bytes.extend_from_slice(&256i16.to_le_bytes());
        bytes.extend_from_slice(&[0xde, 0xad]);
        let block = parse_range(ParamLayout::Two, &bytes).unwrap();
        assert_eq!(block.signed_bounds(), vec![(-127 * 256, 0)]);
        assert_eq!(block.subranges[0].res, 256);
        assert!(block.check_signed(-20 * 256).is_ok());
        assert!(matches!(
            block.check_signed(256),
            Err(ControlError::OutOfRange { requested: 256, .. })
        ));
    }

    #[test]
    fn range_block_rejects_short_and_empty() {
        assert_eq!(parse_range_count(&[0, 0]), Err(ControlError::EmptyRange));
        assert!(matches!(
            parse_range(ParamLayout::Four, &[1, 0, 0]),
            Err(ControlError::ShortResponse {
                expected: 14,
                actual: 3
            })
        ));
        assert_eq!(ParamLayout::Four.range_length(2), Ok(26));
        assert_eq!(
            ParamLayout::Four.range_length(u16::MAX),
            Err(ControlError::LengthOverflow)
        );
    }

    #[test]
    fn cur_encoding_round_trips() {
        let payload = encode_cur(ParamLayout::Two, (-20i16 * 256) as u16 as u32);
        assert_eq!(payload, vec![0x00, 0xec]);
        let raw = parse_cur(ParamLayout::Two, &payload).unwrap();
        assert_eq!(ParamLayout::Two.signed(raw), -20 * 256);
        assert!(parse_cur(ParamLayout::Four, &[1, 2]).is_err());
    }

    #[test]
    fn feature_controls_map_selectors_and_capabilities() {
        assert_eq!(FeatureControl::try_from(0x02), Ok(FeatureControl::Volume));
        assert_eq!(FeatureControl::try_from(0x10), Err(0x10));
        // volume host programmable (bits 3..2 = 0b11), mute absent
        let bitmap = 0b1100;
        assert_eq!(
            ControlCapability::from_bitmap(bitmap, 1),
            ControlCapability::HostProgrammable
        );
        assert_eq!(
            ControlCapability::from_bitmap(bitmap, 0),
            ControlCapability::Absent
        );
        assert_eq!(
            ControlCapability::from_bitmap(0b10, 0),
            ControlCapability::Invalid
        );
    }
}
