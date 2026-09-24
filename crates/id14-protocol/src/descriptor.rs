//! Interpretation of the device's configuration descriptor: which interface
//! carries control requests, and which entities (units, terminals, clock
//! entities) the AudioControl interface declares.
//!
//! Entity ids are always read here at run time; nothing in this module
//! derives an id from an extension code or a table.

use crate::control_error::ControlError;
use crate::uac2::{ControlCapability, FeatureControl};

/// USB interface class: Audio.
pub const CLASS_AUDIO: u8 = 0x01;
/// Audio interface subclass: AudioControl.
pub const SUBCLASS_AUDIO_CONTROL: u8 = 0x01;
/// AudioControl `bInterfaceProtocol` for UAC2.
pub const PROTOCOL_UAC2: u8 = 0x20;
/// USB interface class: application specific (DFU lives here).
pub const CLASS_DFU: u8 = 0xFE;
/// `bDescriptorType` of class-specific interface descriptors.
pub const CS_INTERFACE: u8 = 0x24;

/// One interface alternate setting, host-independent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceInfo {
    /// `bInterfaceNumber`.
    pub number: u8,
    /// `bAlternateSetting`.
    pub alt_setting: u8,
    /// `bInterfaceClass`.
    pub class: u8,
    /// `bInterfaceSubClass`.
    pub subclass: u8,
    /// `bInterfaceProtocol`.
    pub protocol: u8,
    /// Class-specific descriptor bytes following the interface descriptor.
    pub extra: Vec<u8>,
}

/// The interface that class control requests are addressed to and the only
/// interface the tool claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlInterface {
    /// Interface number (goes into the low byte of `wIndex`).
    pub number: u8,
}

/// Find the DFU interface (class 0xFE). Missing is an error; there is no
/// fallback to interface 0. Interfaces 0-2 are refused as control interface
/// because they must never be claimed.
pub fn find_control_interface(
    interfaces: &[InterfaceInfo],
) -> Result<ControlInterface, ControlError> {
    let mut numbers: Vec<u8> = interfaces
        .iter()
        .filter(|i| i.class == CLASS_DFU)
        .map(|i| i.number)
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    match numbers.as_slice() {
        [] => Err(ControlError::DfuInterfaceMissing),
        [number] if *number <= 2 => Err(ControlError::ControlInterfaceReserved { number: *number }),
        [number] => Ok(ControlInterface { number: *number }),
        _ => Err(ControlError::DfuInterfaceAmbiguous { numbers }),
    }
}

/// Clock Source entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClockSource {
    /// `bClockID`.
    pub id: u8,
    /// `bmControls` (D1..0 frequency, D3..2 validity).
    pub controls: u8,
}

/// Clock Selector entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClockSelector {
    /// `bClockID`.
    pub id: u8,
    /// `baCSourceID`.
    pub sources: Vec<u8>,
    /// `bmControls` (D1..0 selector).
    pub controls: u8,
}

/// Mixer Unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MixerUnit {
    /// `bUnitID`.
    pub id: u8,
    /// `baSourceID`.
    pub sources: Vec<u8>,
    /// `bNrChannels` (logical output channels, `m`).
    pub output_channels: u8,
    /// `bmMixerControls`, row-major, MSb of the first byte = input 1 / output 1.
    pub mixer_controls: Vec<u8>,
}

impl MixerUnit {
    /// Whether the node (input `u`, output `v`; both 1-based) is programmable.
    pub fn is_programmable(&self, u: u16, v: u8) -> bool {
        let m = self.output_channels as usize;
        if u == 0 || v == 0 {
            return false;
        }
        let idx = (u as usize - 1) * m + (v as usize - 1);
        self.mixer_controls
            .get(idx / 8)
            .is_some_and(|byte| byte & (0x80 >> (idx % 8)) != 0)
    }
}

/// Feature Unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FeatureUnit {
    /// `bUnitID`.
    pub id: u8,
    /// `bSourceID`.
    pub source: u8,
    /// `bmaControls(0..=ch)`; index 0 is the master channel.
    pub controls: Vec<u32>,
}

impl FeatureUnit {
    /// Capability of `control` on `channel` (0 = master).
    pub fn capability(&self, channel: u8, control: FeatureControl) -> ControlCapability {
        match self.controls.get(channel as usize) {
            Some(&bitmap) => {
                ControlCapability::from_bitmap(bitmap, u32::from(control.selector()) - 1)
            }
            None => ControlCapability::Absent,
        }
    }

    /// Number of logical channels (excluding master).
    pub fn channel_count(&self) -> u8 {
        u8::try_from(self.controls.len().saturating_sub(1)).unwrap_or(u8::MAX)
    }

    /// Whether any channel declares `control`.
    pub fn declares(&self, control: FeatureControl) -> bool {
        (0..self.controls.len())
            .any(|ch| u8::try_from(ch).is_ok_and(|c| self.capability(c, control).is_declared()))
    }
}

/// Extension Unit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtensionUnit {
    /// `bUnitID`.
    pub id: u8,
    /// `wExtensionCode` — the wire field that carries the app's extension code.
    pub extension_code: u16,
    /// `baSourceID`.
    pub sources: Vec<u8>,
    /// `bNrChannels`.
    pub output_channels: u8,
}

/// One AudioControl entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Entity {
    /// Input Terminal.
    InputTerminal {
        /// `bTerminalID`.
        id: u8,
        /// `bNrChannels`.
        channels: u8,
    },
    /// Output Terminal.
    OutputTerminal {
        /// `bTerminalID`.
        id: u8,
        /// `bSourceID`.
        source: u8,
    },
    /// Mixer Unit.
    Mixer(MixerUnit),
    /// Selector Unit.
    Selector {
        /// `bUnitID`.
        id: u8,
        /// `baSourceID`.
        sources: Vec<u8>,
    },
    /// Feature Unit.
    Feature(FeatureUnit),
    /// Effect Unit or Sampling Rate Converter (single source, channel count
    /// passes through).
    PassThrough {
        /// `bDescriptorSubtype`.
        subtype: u8,
        /// Unit id.
        id: u8,
        /// `bSourceID`.
        source: u8,
    },
    /// Processing Unit.
    Processing {
        /// `bUnitID`.
        id: u8,
        /// `baSourceID`.
        sources: Vec<u8>,
        /// `bNrChannels`.
        channels: u8,
    },
    /// Extension Unit.
    Extension(ExtensionUnit),
    /// Clock Source.
    ClockSource(ClockSource),
    /// Clock Selector.
    ClockSelector(ClockSelector),
    /// Clock Multiplier.
    ClockMultiplier {
        /// `bClockID`.
        id: u8,
    },
}

impl Entity {
    /// Entity id.
    pub fn id(&self) -> u8 {
        match self {
            Entity::InputTerminal { id, .. }
            | Entity::OutputTerminal { id, .. }
            | Entity::Selector { id, .. }
            | Entity::PassThrough { id, .. }
            | Entity::Processing { id, .. }
            | Entity::ClockMultiplier { id } => *id,
            Entity::Mixer(m) => m.id,
            Entity::Feature(f) => f.id,
            Entity::Extension(x) => x.id,
            Entity::ClockSource(c) => c.id,
            Entity::ClockSelector(c) => c.id,
        }
    }

    /// Ids this entity takes audio from.
    pub fn audio_sources(&self) -> Vec<u8> {
        match self {
            Entity::OutputTerminal { source, .. } | Entity::PassThrough { source, .. } => {
                vec![*source]
            }
            Entity::Feature(f) => vec![f.source],
            Entity::Mixer(m) => m.sources.clone(),
            Entity::Selector { sources, .. } | Entity::Processing { sources, .. } => {
                sources.clone()
            }
            Entity::Extension(x) => x.sources.clone(),
            _ => Vec::new(),
        }
    }
}

/// Parsed AudioControl interface.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioControl {
    /// AudioControl interface number (never claimed by this tool).
    pub interface_number: u8,
    /// Entities in descriptor order.
    pub entities: Vec<Entity>,
}

const SUBTYPE_HEADER: u8 = 0x01;
const SUBTYPE_INPUT_TERMINAL: u8 = 0x02;
const SUBTYPE_OUTPUT_TERMINAL: u8 = 0x03;
const SUBTYPE_MIXER_UNIT: u8 = 0x04;
const SUBTYPE_SELECTOR_UNIT: u8 = 0x05;
const SUBTYPE_FEATURE_UNIT: u8 = 0x06;
const SUBTYPE_EFFECT_UNIT: u8 = 0x07;
const SUBTYPE_PROCESSING_UNIT: u8 = 0x08;
const SUBTYPE_EXTENSION_UNIT: u8 = 0x09;
const SUBTYPE_CLOCK_SOURCE: u8 = 0x0A;
const SUBTYPE_CLOCK_SELECTOR: u8 = 0x0B;
const SUBTYPE_CLOCK_MULTIPLIER: u8 = 0x0C;
const SUBTYPE_SAMPLE_RATE_CONVERTER: u8 = 0x0D;

impl AudioControl {
    /// Locate the UAC2 AudioControl interface and parse its entities.
    pub fn from_interfaces(interfaces: &[InterfaceInfo]) -> Result<Self, ControlError> {
        let ac = interfaces
            .iter()
            .find(|i| {
                i.class == CLASS_AUDIO && i.subclass == SUBCLASS_AUDIO_CONTROL && i.alt_setting == 0
            })
            .ok_or(ControlError::AudioControlInterfaceMissing)?;
        if ac.protocol != PROTOCOL_UAC2 {
            return Err(ControlError::UnsupportedAudioClass {
                protocol: ac.protocol,
            });
        }
        Ok(AudioControl {
            interface_number: ac.number,
            entities: parse_entities(&ac.extra)?,
        })
    }

    /// Entity with `id`.
    pub fn entity(&self, id: u8) -> Result<&Entity, ControlError> {
        self.entities
            .iter()
            .find(|e| e.id() == id)
            .ok_or(ControlError::UnknownEntity(id))
    }

    /// Feature Units in descriptor order.
    pub fn feature_units(&self) -> impl Iterator<Item = &FeatureUnit> {
        self.entities.iter().filter_map(|e| match e {
            Entity::Feature(f) => Some(f),
            _ => None,
        })
    }

    /// Mixer Units in descriptor order.
    pub fn mixer_units(&self) -> impl Iterator<Item = &MixerUnit> {
        self.entities.iter().filter_map(|e| match e {
            Entity::Mixer(m) => Some(m),
            _ => None,
        })
    }

    /// Extension Units in descriptor order.
    pub fn extension_units(&self) -> impl Iterator<Item = &ExtensionUnit> {
        self.entities.iter().filter_map(|e| match e {
            Entity::Extension(x) => Some(x),
            _ => None,
        })
    }

    /// Logical channel count of the cluster leaving entity `id`.
    pub fn output_channels(&self, id: u8) -> Result<u16, ControlError> {
        self.output_channels_bounded(id, self.entities.len() + 1)
    }

    fn output_channels_bounded(&self, id: u8, budget: usize) -> Result<u16, ControlError> {
        if budget == 0 {
            return Err(ControlError::ChannelCountUnresolved(id));
        }
        match self.entity(id)? {
            Entity::InputTerminal { channels, .. } | Entity::Processing { channels, .. } => {
                Ok(u16::from(*channels))
            }
            Entity::Mixer(m) => Ok(u16::from(m.output_channels)),
            Entity::Extension(x) => Ok(u16::from(x.output_channels)),
            Entity::Feature(f) => self.output_channels_bounded(f.source, budget - 1),
            Entity::PassThrough { source, .. } => self.output_channels_bounded(*source, budget - 1),
            Entity::Selector { sources, .. } => match sources.first() {
                Some(&first) => self.output_channels_bounded(first, budget - 1),
                None => Err(ControlError::ChannelCountUnresolved(id)),
            },
            _ => Err(ControlError::ChannelCountUnresolved(id)),
        }
    }

    /// Logical input channel count (`n`) of a mixer unit, checked against the
    /// length of its `bmMixerControls`.
    pub fn mixer_input_channels(&self, mixer: &MixerUnit) -> Result<u16, ControlError> {
        let mut n: u16 = 0;
        for &source in &mixer.sources {
            n = n
                .checked_add(self.output_channels(source)?)
                .ok_or(ControlError::ChannelCountUnresolved(mixer.id))?;
        }
        let nodes = usize::from(n) * usize::from(mixer.output_channels);
        if nodes > 256 || nodes.div_ceil(8) != mixer.mixer_controls.len() {
            return Err(ControlError::MixerBitmapMismatch {
                unit: mixer.id,
                inputs: n,
                outputs: mixer.output_channels,
            });
        }
        Ok(n)
    }

    /// The Feature Unit on the output side of the mixer unit(s) that declares
    /// a volume control: walk downstream from every mixer unit, stop at the
    /// nearest Feature Units declaring volume. Exactly one must qualify.
    pub fn mixer_output_feature_unit(&self) -> Result<&FeatureUnit, ControlError> {
        let mut found: Vec<u8> = Vec::new();
        for mixer in self.mixer_units() {
            let mut frontier = vec![mixer.id];
            let mut visited = vec![mixer.id];
            while !frontier.is_empty() {
                let mut next = Vec::new();
                let mut level_hits = Vec::new();
                for entity in &self.entities {
                    let id = entity.id();
                    if visited.contains(&id)
                        || !entity.audio_sources().iter().any(|s| frontier.contains(s))
                    {
                        continue;
                    }
                    visited.push(id);
                    match entity {
                        Entity::Feature(f) if f.declares(FeatureControl::Volume) => {
                            level_hits.push(f.id)
                        }
                        Entity::OutputTerminal { .. } => {}
                        _ => next.push(id),
                    }
                }
                if !level_hits.is_empty() {
                    found.extend(level_hits);
                    break;
                }
                frontier = next;
            }
        }
        found.sort_unstable();
        found.dedup();
        match found.as_slice() {
            [] => Err(ControlError::MixerOutputFeatureUnitMissing),
            [id] => match self.entity(*id)? {
                Entity::Feature(f) => Ok(f),
                _ => Err(ControlError::UnknownEntity(*id)),
            },
            _ => Err(ControlError::MixerOutputFeatureUnitAmbiguous(found)),
        }
    }

    /// Every `(feature unit id, channel)` that declares a mute control.
    pub fn declared_mute_controls(&self) -> Vec<(u8, u8)> {
        self.feature_units()
            .flat_map(|f| {
                (0..=f.channel_count())
                    .filter(|&ch| f.capability(ch, FeatureControl::Mute).is_declared())
                    .map(move |ch| (f.id, ch))
            })
            .collect()
    }
}

fn byte(desc: &[u8], idx: usize, subtype: u8, offset: usize) -> Result<u8, ControlError> {
    desc.get(idx)
        .copied()
        .ok_or(ControlError::EntityMalformed { subtype, offset })
}

fn slice(
    desc: &[u8],
    start: usize,
    len: usize,
    subtype: u8,
    offset: usize,
) -> Result<Vec<u8>, ControlError> {
    desc.get(start..start + len)
        .map(<[u8]>::to_vec)
        .ok_or(ControlError::EntityMalformed { subtype, offset })
}

fn parse_entities(extra: &[u8]) -> Result<Vec<Entity>, ControlError> {
    let mut entities = Vec::new();
    let mut offset = 0;
    while offset < extra.len() {
        let len = usize::from(extra[offset]);
        if len < 2 {
            return Err(ControlError::DescriptorLengthInvalid { offset });
        }
        let desc = extra
            .get(offset..offset + len)
            .ok_or(ControlError::DescriptorTruncated { offset })?;
        if desc[1] == CS_INTERFACE && len >= 3 {
            if let Some(entity) = parse_entity(desc, offset)? {
                entities.push(entity);
            }
        }
        offset += len;
    }
    Ok(entities)
}

fn parse_entity(desc: &[u8], offset: usize) -> Result<Option<Entity>, ControlError> {
    let subtype = desc[2];
    let b = |idx: usize| byte(desc, idx, subtype, offset);
    let entity = match subtype {
        SUBTYPE_HEADER => return Ok(None),
        SUBTYPE_INPUT_TERMINAL => Entity::InputTerminal {
            id: b(3)?,
            channels: b(8)?,
        },
        SUBTYPE_OUTPUT_TERMINAL => Entity::OutputTerminal {
            id: b(3)?,
            source: b(7)?,
        },
        SUBTYPE_MIXER_UNIT => {
            let p = usize::from(b(4)?);
            // bLength = 13 + p + N
            let n = desc
                .len()
                .checked_sub(13 + p)
                .ok_or(ControlError::EntityMalformed { subtype, offset })?;
            Entity::Mixer(MixerUnit {
                id: b(3)?,
                sources: slice(desc, 5, p, subtype, offset)?,
                output_channels: b(5 + p)?,
                mixer_controls: slice(desc, 11 + p, n, subtype, offset)?,
            })
        }
        SUBTYPE_SELECTOR_UNIT => {
            let p = usize::from(b(4)?);
            Entity::Selector {
                id: b(3)?,
                sources: slice(desc, 5, p, subtype, offset)?,
            }
        }
        SUBTYPE_FEATURE_UNIT => {
            // bLength = 6 + (ch + 1) * 4
            let body = desc
                .len()
                .checked_sub(6)
                .filter(|n| *n >= 4 && n % 4 == 0)
                .ok_or(ControlError::EntityMalformed { subtype, offset })?;
            let controls = slice(desc, 5, body, subtype, offset)?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
            Entity::Feature(FeatureUnit {
                id: b(3)?,
                source: b(4)?,
                controls,
            })
        }
        SUBTYPE_EFFECT_UNIT => Entity::PassThrough {
            subtype,
            id: b(3)?,
            source: b(6)?,
        },
        SUBTYPE_SAMPLE_RATE_CONVERTER => Entity::PassThrough {
            subtype,
            id: b(3)?,
            source: b(4)?,
        },
        SUBTYPE_PROCESSING_UNIT => {
            let p = usize::from(b(6)?);
            Entity::Processing {
                id: b(3)?,
                sources: slice(desc, 7, p, subtype, offset)?,
                channels: b(7 + p)?,
            }
        }
        SUBTYPE_EXTENSION_UNIT => {
            let p = usize::from(b(6)?);
            Entity::Extension(ExtensionUnit {
                id: b(3)?,
                extension_code: u16::from_le_bytes([b(4)?, b(5)?]),
                sources: slice(desc, 7, p, subtype, offset)?,
                output_channels: b(7 + p)?,
            })
        }
        SUBTYPE_CLOCK_SOURCE => Entity::ClockSource(ClockSource {
            id: b(3)?,
            controls: b(5)?,
        }),
        SUBTYPE_CLOCK_SELECTOR => {
            let p = usize::from(b(4)?);
            Entity::ClockSelector(ClockSelector {
                id: b(3)?,
                sources: slice(desc, 5, p, subtype, offset)?,
                controls: b(5 + p)?,
            })
        }
        SUBTYPE_CLOCK_MULTIPLIER => Entity::ClockMultiplier { id: b(3)? },
        _ => return Ok(None),
    };
    Ok(Some(entity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, mk2_synthetic_interfaces};

    fn iface(number: u8, class: u8) -> InterfaceInfo {
        InterfaceInfo {
            number,
            alt_setting: 0,
            class,
            subclass: 0,
            protocol: 0,
            extra: Vec::new(),
        }
    }

    #[test]
    fn dfu_interface_is_found_from_descriptor() {
        let found = find_control_interface(&mk2_synthetic_interfaces()).unwrap();
        assert_eq!(found.number, fixtures::MK2_DFU_INTERFACE);
    }

    #[test]
    fn missing_dfu_interface_is_an_error_without_interface_0_fallback() {
        let ifaces = vec![iface(0, CLASS_AUDIO), iface(1, CLASS_AUDIO), iface(3, 0x03)];
        assert_eq!(
            find_control_interface(&ifaces),
            Err(ControlError::DfuInterfaceMissing)
        );
    }

    #[test]
    fn dfu_interface_numbered_0_to_2_is_refused() {
        for n in 0..=2 {
            assert_eq!(
                find_control_interface(&[iface(n, CLASS_DFU)]),
                Err(ControlError::ControlInterfaceReserved { number: n })
            );
        }
    }

    #[test]
    fn ambiguous_dfu_interfaces_are_refused() {
        let ifaces = vec![iface(4, CLASS_DFU), iface(5, CLASS_DFU)];
        assert!(matches!(
            find_control_interface(&ifaces),
            Err(ControlError::DfuInterfaceAmbiguous { .. })
        ));
    }

    #[test]
    fn non_uac2_audio_control_is_refused() {
        let mut ifaces = mk2_synthetic_interfaces();
        ifaces[0].protocol = 0x00;
        assert_eq!(
            AudioControl::from_interfaces(&ifaces),
            Err(ControlError::UnsupportedAudioClass { protocol: 0 })
        );
    }

    #[test]
    fn truncated_and_zero_length_descriptors_are_errors() {
        let mut ifaces = mk2_synthetic_interfaces();
        ifaces[0].extra = vec![9, CS_INTERFACE, 1];
        assert_eq!(
            AudioControl::from_interfaces(&ifaces),
            Err(ControlError::DescriptorTruncated { offset: 0 })
        );
        ifaces[0].extra = vec![0, 0];
        assert_eq!(
            AudioControl::from_interfaces(&ifaces),
            Err(ControlError::DescriptorLengthInvalid { offset: 0 })
        );
    }

    #[test]
    fn mk2_fixture_entities_parse() {
        let ac = AudioControl::from_interfaces(&mk2_synthetic_interfaces()).unwrap();
        let mixer = ac.mixer_units().next().unwrap();
        assert_eq!(mixer.id, fixtures::MK2_MIXER_UNIT);
        assert_eq!(mixer.output_channels, 6);
        assert_eq!(ac.mixer_input_channels(mixer), Ok(16));
        assert!(mixer.is_programmable(1, 1));
        let fu = ac.mixer_output_feature_unit().unwrap();
        assert_eq!(fu.id, fixtures::MK2_VOLUME_FEATURE_UNIT);
        assert!(ac.declared_mute_controls().is_empty());
    }

    #[test]
    fn extension_code_is_the_wire_field_not_the_unit_id() {
        let ac = AudioControl::from_interfaces(&mk2_synthetic_interfaces()).unwrap();
        let code_of = |id: u8| {
            ac.extension_units()
                .find(|x| x.id == id)
                .map(|x| x.extension_code)
        };
        assert_eq!(code_of(fixtures::MK2_MONITOR_CONTROLLER_XU), Some(1));
        assert_eq!(code_of(fixtures::MK2_MONO_MIX_CONTROLLER_XU), Some(2));
        for x in ac.extension_units() {
            assert_ne!(u16::from(x.id), x.extension_code);
        }
    }

    #[test]
    fn missing_mixer_output_volume_is_an_error() {
        let mut ac = AudioControl::from_interfaces(&mk2_synthetic_interfaces()).unwrap();
        for e in &mut ac.entities {
            if let Entity::Feature(f) = e {
                if f.id == fixtures::MK2_VOLUME_FEATURE_UNIT {
                    f.controls.iter_mut().for_each(|c| *c = 0);
                }
            }
        }
        assert_eq!(
            ac.mixer_output_feature_unit(),
            Err(ControlError::MixerOutputFeatureUnitMissing)
        );
    }
}
