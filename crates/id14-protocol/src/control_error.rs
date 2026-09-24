//! Explicit error representation for UAC2 descriptor interpretation, request
//! building and response decoding. Nothing here panics on bad input.

use core::fmt;

/// Error returned by the UAC2 control-path operations of this crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ControlError {
    /// A class-specific descriptor runs past the end of the buffer.
    DescriptorTruncated {
        /// Byte offset of the descriptor inside the interface's extra bytes.
        offset: usize,
    },
    /// A descriptor declares `bLength` 0 or 1 (cannot advance).
    DescriptorLengthInvalid {
        /// Byte offset of the descriptor inside the interface's extra bytes.
        offset: usize,
    },
    /// An entity descriptor is shorter than its own layout requires.
    EntityMalformed {
        /// `bDescriptorSubtype` of the descriptor.
        subtype: u8,
        /// Byte offset of the descriptor inside the interface's extra bytes.
        offset: usize,
    },
    /// No AudioControl interface (class 0x01, subclass 0x01) was found.
    AudioControlInterfaceMissing,
    /// The AudioControl interface is not USB Audio Class 2.0.
    UnsupportedAudioClass {
        /// `bInterfaceProtocol` of the AudioControl interface.
        protocol: u8,
    },
    /// No DFU interface (class 0xFE) was found. There is deliberately no
    /// fallback to interface 0.
    DfuInterfaceMissing,
    /// More than one distinct interface number carries class 0xFE.
    DfuInterfaceAmbiguous {
        /// The candidate interface numbers.
        numbers: Vec<u8>,
    },
    /// The DFU interface is numbered 0-2, which must never be claimed.
    ControlInterfaceReserved {
        /// The offending interface number.
        number: u8,
    },
    /// An entity id is referenced but not described.
    UnknownEntity(u8),
    /// The channel count of an entity's output cluster cannot be derived
    /// (cycle, clock entity or output terminal in the audio path).
    ChannelCountUnresolved(u8),
    /// A mixer unit's `bmMixerControls` length does not match `n * m`.
    MixerBitmapMismatch {
        /// Mixer unit id.
        unit: u8,
        /// Logical input channels (`n`).
        inputs: u16,
        /// Logical output channels (`m`).
        outputs: u8,
    },
    /// No Feature Unit with a declared volume control sits on the output side
    /// of a mixer unit.
    MixerOutputFeatureUnitMissing,
    /// More than one Feature Unit qualifies as the mixer's output-side unit.
    MixerOutputFeatureUnitAmbiguous(Vec<u8>),
    /// The requested channel does not declare a volume control.
    VolumeNotDeclared {
        /// Feature Unit id.
        unit: u8,
        /// Channel number (0 = master).
        channel: u8,
    },
    /// The requested channel declares a read-only volume control.
    VolumeNotWritable {
        /// Feature Unit id.
        unit: u8,
        /// Channel number (0 = master).
        channel: u8,
    },
    /// The device answered with fewer bytes than the parameter block needs.
    ShortResponse {
        /// Bytes required.
        expected: usize,
        /// Bytes received.
        actual: usize,
    },
    /// A RANGE parameter block reports zero subranges.
    EmptyRange,
    /// A computed `wLength` does not fit in 16 bits.
    LengthOverflow,
    /// The requested value lies outside every subrange the device reported.
    OutOfRange {
        /// Requested value in the control's raw unit.
        requested: i64,
        /// Reported `(min, max)` pairs in the same unit.
        ranges: Vec<(i64, i64)>,
    },
    /// A decibel argument could not be parsed or is not representable.
    InvalidDecibels(String),
}

impl ControlError {
    /// Stable machine-readable code for the error kind.
    pub const fn code(&self) -> &'static str {
        match self {
            ControlError::DescriptorTruncated { .. } => "descriptor_truncated",
            ControlError::DescriptorLengthInvalid { .. } => "descriptor_length_invalid",
            ControlError::EntityMalformed { .. } => "entity_malformed",
            ControlError::AudioControlInterfaceMissing => "audio_control_interface_missing",
            ControlError::UnsupportedAudioClass { .. } => "unsupported_audio_class",
            ControlError::DfuInterfaceMissing => "dfu_interface_missing",
            ControlError::DfuInterfaceAmbiguous { .. } => "dfu_interface_ambiguous",
            ControlError::ControlInterfaceReserved { .. } => "control_interface_reserved",
            ControlError::UnknownEntity(_) => "unknown_entity",
            ControlError::ChannelCountUnresolved(_) => "channel_count_unresolved",
            ControlError::MixerBitmapMismatch { .. } => "mixer_bitmap_mismatch",
            ControlError::MixerOutputFeatureUnitMissing => "mixer_output_feature_unit_missing",
            ControlError::MixerOutputFeatureUnitAmbiguous(_) => {
                "mixer_output_feature_unit_ambiguous"
            }
            ControlError::VolumeNotDeclared { .. } => "volume_not_declared",
            ControlError::VolumeNotWritable { .. } => "volume_not_writable",
            ControlError::ShortResponse { .. } => "short_response",
            ControlError::EmptyRange => "empty_range",
            ControlError::LengthOverflow => "length_overflow",
            ControlError::OutOfRange { .. } => "out_of_range",
            ControlError::InvalidDecibels(_) => "invalid_decibels",
        }
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlError::DescriptorTruncated { offset } => {
                write!(f, "class-specific descriptor at offset {offset} is truncated")
            }
            ControlError::DescriptorLengthInvalid { offset } => {
                write!(f, "descriptor at offset {offset} has an invalid bLength")
            }
            ControlError::EntityMalformed { subtype, offset } => write!(
                f,
                "entity descriptor (subtype {subtype:#04x}) at offset {offset} is malformed"
            ),
            ControlError::AudioControlInterfaceMissing => {
                f.write_str("no AudioControl interface in the active configuration")
            }
            ControlError::UnsupportedAudioClass { protocol } => write!(
                f,
                "AudioControl interface protocol {protocol:#04x} is not UAC2 (0x20)"
            ),
            ControlError::DfuInterfaceMissing => f.write_str(
                "no DFU interface (class 0xfe) in the descriptor; refusing to fall back to interface 0",
            ),
            ControlError::DfuInterfaceAmbiguous { numbers } => {
                write!(f, "more than one DFU interface (class 0xfe): {numbers:?}")
            }
            ControlError::ControlInterfaceReserved { number } => write!(
                f,
                "DFU interface is numbered {number}; interfaces 0-2 are never claimed"
            ),
            ControlError::UnknownEntity(id) => {
                write!(f, "entity {id:#04x} is referenced but not described")
            }
            ControlError::ChannelCountUnresolved(id) => {
                write!(f, "cannot derive the channel count of entity {id:#04x}")
            }
            ControlError::MixerBitmapMismatch {
                unit,
                inputs,
                outputs,
            } => write!(
                f,
                "mixer unit {unit:#04x}: bmMixerControls length does not match {inputs} x {outputs}"
            ),
            ControlError::MixerOutputFeatureUnitMissing => f.write_str(
                "no Feature Unit with a declared volume control on the output side of a mixer unit",
            ),
            ControlError::MixerOutputFeatureUnitAmbiguous(ids) => write!(
                f,
                "more than one mixer-output Feature Unit declares volume: {ids:02x?}"
            ),
            ControlError::VolumeNotDeclared { unit, channel } => write!(
                f,
                "Feature Unit {unit:#04x} declares no volume control on channel {channel}"
            ),
            ControlError::VolumeNotWritable { unit, channel } => write!(
                f,
                "Feature Unit {unit:#04x} volume on channel {channel} is read-only"
            ),
            ControlError::ShortResponse { expected, actual } => write!(
                f,
                "device answered {actual} byte(s); the parameter block needs {expected}"
            ),
            ControlError::EmptyRange => f.write_str("RANGE reports zero subranges"),
            ControlError::LengthOverflow => f.write_str("request length exceeds 65535 bytes"),
            ControlError::OutOfRange { requested, ranges } => write!(
                f,
                "value {requested} is outside the device's RANGE {ranges:?}; not sent"
            ),
            ControlError::InvalidDecibels(input) => {
                write!(f, "invalid decibel value {input:?}")
            }
        }
    }
}

impl std::error::Error for ControlError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dfu_missing_message_states_no_interface_0_fallback() {
        let text = ControlError::DfuInterfaceMissing.to_string();
        assert!(text.contains("refusing to fall back to interface 0"));
        assert_eq!(
            ControlError::DfuInterfaceMissing.code(),
            "dfu_interface_missing"
        );
    }
}
