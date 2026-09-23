//! Pure request planning for the CLI operations: which read-only requests
//! `dump` sends, and which requests `volume` sends. Nothing here performs
//! I/O; the plans are executed (or only printed, for `--dry-run`) by the host
//! layer.

use crate::control_error::ControlError;
use crate::db::format_raw_db;
use crate::descriptor::{AudioControl, ControlInterface, Entity};
use crate::uac2::{
    encode_cur, ControlAddress, ControlCapability, FeatureControl, ParamLayout, SetupPacket,
    ValueKind, CS_SAM_FREQ_CONTROL, CX_CLOCK_SELECTOR_CONTROL, MU_MIXER_CONTROL,
};

/// What one dump read addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DumpTarget {
    /// Clock Source sampling frequency.
    ClockSourceSamFreq {
        /// Clock Source id.
        id: u8,
    },
    /// Clock Selector input pin.
    ClockSelector {
        /// Clock Selector id.
        id: u8,
    },
    /// A declared Feature Unit control on one channel.
    FeatureUnit {
        /// Feature Unit id.
        id: u8,
        /// Channel (0 = master).
        channel: u8,
        /// Control.
        control: FeatureControl,
    },
    /// One mixer node.
    MixerNode {
        /// Mixer Unit id.
        id: u8,
        /// Logical input channel (1-based).
        input: u16,
        /// Logical output channel (1-based).
        output: u8,
        /// Whether the node is programmable (`bmMixerControls`).
        programmable: bool,
    },
}

/// One read-only dump step: GET CUR, optionally preceded by GET RANGE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DumpRead {
    /// Addressed control.
    pub target: DumpTarget,
    /// Setup address.
    pub address: ControlAddress,
    /// `wLength` of GET CUR.
    pub cur_length: u16,
    /// Parameter layout (`None` = variable-length block).
    pub layout: Option<ParamLayout>,
    /// Interpretation of the value.
    pub kind: ValueKind,
    /// Whether GET RANGE is read as well.
    pub read_range: bool,
}

impl DumpRead {
    /// Human-readable label.
    pub fn label(&self) -> String {
        match self.target {
            DumpTarget::ClockSourceSamFreq { id } => format!("clock source {id:#04x} SAM_FREQ"),
            DumpTarget::ClockSelector { id } => format!("clock selector {id:#04x} SELECTOR"),
            DumpTarget::FeatureUnit {
                id,
                channel,
                control,
            } => format!("feature unit {id:#04x} ch{channel} {}", control.name()),
            DumpTarget::MixerNode {
                id,
                input,
                output,
                programmable,
            } => format!(
                "mixer unit {id:#04x} in{input}->out{output} (MCN {}){}",
                self.address.channel,
                if programmable { "" } else { " [fixed]" }
            ),
        }
    }

    /// GET CUR setup packet.
    pub fn cur_packet(&self) -> SetupPacket {
        SetupPacket::get_cur(self.address, self.cur_length)
    }

    /// First GET RANGE packet (reads `wNumSubRanges`), when a range is read.
    pub fn range_count_packet(&self) -> Option<SetupPacket> {
        self.read_range
            .then(|| SetupPacket::get_range(self.address, ParamLayout::range_count_length()))
    }

    /// Second GET RANGE packet for `subranges` entries.
    pub fn range_packet(&self, subranges: u16) -> Result<Option<SetupPacket>, ControlError> {
        match (self.read_range, self.layout) {
            (true, Some(layout)) => Ok(Some(SetupPacket::get_range(
                self.address,
                layout.range_length(subranges)?,
            ))),
            _ => Ok(None),
        }
    }

    /// Every packet the plan can know without device answers.
    pub fn known_packets(&self) -> Vec<SetupPacket> {
        let mut packets: Vec<SetupPacket> = self.range_count_packet().into_iter().collect();
        packets.push(self.cur_packet());
        packets
    }
}

/// The complete read-only dump plan.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DumpPlan {
    /// Control interface (DFU interface) in `wIndex`.
    pub control_interface: u8,
    /// Reads in descriptor order.
    pub reads: Vec<DumpRead>,
    /// Entities that could not be planned, with the reason.
    pub skipped: Vec<String>,
}

/// Plan the read-only dump: clock source SAM_FREQ, clock selector, every
/// declared Feature Unit control on every channel, and every mixer node.
/// Extension Unit controls are never read. Only GET CUR / GET RANGE are
/// planned.
pub fn dump_plan(ac: &AudioControl, control: ControlInterface) -> DumpPlan {
    let iface = control.number;
    let mut reads = Vec::new();
    let mut skipped = Vec::new();
    let addr = |entity_id: u8, control_selector: u8, channel: u8| ControlAddress {
        entity_id,
        control_selector,
        channel,
        interface_number: iface,
    };
    for entity in &ac.entities {
        match entity {
            Entity::ClockSource(c)
                if ControlCapability::from_bitmap(u32::from(c.controls), 0).is_declared() =>
            {
                reads.push(DumpRead {
                    target: DumpTarget::ClockSourceSamFreq { id: c.id },
                    address: addr(c.id, CS_SAM_FREQ_CONTROL, 0),
                    cur_length: ParamLayout::Four.cur_length(),
                    layout: Some(ParamLayout::Four),
                    kind: ValueKind::Hertz,
                    read_range: true,
                });
            }
            Entity::ClockSelector(c)
                if ControlCapability::from_bitmap(u32::from(c.controls), 0).is_declared() =>
            {
                reads.push(DumpRead {
                    target: DumpTarget::ClockSelector { id: c.id },
                    address: addr(c.id, CX_CLOCK_SELECTOR_CONTROL, 0),
                    cur_length: ParamLayout::One.cur_length(),
                    layout: Some(ParamLayout::One),
                    kind: ValueKind::SelectorPin,
                    read_range: false,
                });
            }
            Entity::Feature(f) => {
                for channel in 0..=f.channel_count() {
                    for control in FeatureControl::ALL {
                        if f.capability(channel, control).is_declared() {
                            reads.push(DumpRead {
                                target: DumpTarget::FeatureUnit {
                                    id: f.id,
                                    channel,
                                    control,
                                },
                                address: addr(f.id, control.selector(), channel),
                                cur_length: control.cur_length(),
                                layout: control.layout(),
                                kind: control.value_kind(),
                                read_range: control == FeatureControl::Volume,
                            });
                        }
                    }
                }
            }
            Entity::Mixer(m) => match ac.mixer_input_channels(m) {
                Ok(inputs) => {
                    let outputs = m.output_channels;
                    for u in 1..=inputs {
                        for v in 1..=outputs {
                            let mcn =
                                (usize::from(u) - 1) * usize::from(outputs) + (usize::from(v) - 1);
                            // n * m <= 256 is checked by mixer_input_channels
                            let Ok(mcn) = u8::try_from(mcn) else { continue };
                            reads.push(DumpRead {
                                target: DumpTarget::MixerNode {
                                    id: m.id,
                                    input: u,
                                    output: v,
                                    programmable: m.is_programmable(u, v),
                                },
                                address: addr(m.id, MU_MIXER_CONTROL, mcn),
                                cur_length: ParamLayout::Two.cur_length(),
                                layout: Some(ParamLayout::Two),
                                kind: ValueKind::Decibel256,
                                read_range: false,
                            });
                        }
                    }
                }
                Err(err) => skipped.push(format!("mixer unit {:#04x}: {err}", m.id)),
            },
            _ => {}
        }
    }
    DumpPlan {
        control_interface: iface,
        reads,
        skipped,
    }
}

/// Render a decoded value.
pub fn format_value(
    kind: ValueKind,
    layout: Option<ParamLayout>,
    raw: u32,
    bytes: &[u8],
) -> String {
    match (kind, layout) {
        (ValueKind::Bool, _) => (if raw != 0 { "true" } else { "false" }).to_string(),
        (ValueKind::Decibel256, _) => format_raw_db(raw as u16 as i16),
        (ValueKind::DecibelQuarter, _) => format!("{:.2} dB", f64::from(raw as u8 as i8) / 4.0),
        (ValueKind::Hertz, _) => format!("{raw} Hz"),
        (ValueKind::SelectorPin, _) => format!("input pin {raw}"),
        (ValueKind::Unsigned, _) => raw.to_string(),
        (ValueKind::Raw, _) => bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// The volume control that `volume` writes: the declared volume control of
/// the mixer-output Feature Unit on one channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VolumeTarget {
    /// Feature Unit id (read from the descriptor).
    pub unit: u8,
    /// Channel (0 = master).
    pub channel: u8,
    /// Setup address.
    pub address: ControlAddress,
}

/// Resolve the volume target for `channel`.
pub fn volume_target(
    ac: &AudioControl,
    control: ControlInterface,
    channel: u8,
) -> Result<VolumeTarget, ControlError> {
    let fu = ac.mixer_output_feature_unit()?;
    match fu.capability(channel, FeatureControl::Volume) {
        ControlCapability::HostProgrammable => Ok(VolumeTarget {
            unit: fu.id,
            channel,
            address: ControlAddress {
                entity_id: fu.id,
                control_selector: FeatureControl::Volume.selector(),
                channel,
                interface_number: control.number,
            },
        }),
        ControlCapability::ReadOnly => Err(ControlError::VolumeNotWritable {
            unit: fu.id,
            channel,
        }),
        _ => Err(ControlError::VolumeNotDeclared {
            unit: fu.id,
            channel,
        }),
    }
}

impl VolumeTarget {
    /// GET RANGE reading `wNumSubRanges`.
    pub fn range_count_packet(&self) -> SetupPacket {
        SetupPacket::get_range(self.address, ParamLayout::range_count_length())
    }

    /// GET RANGE reading `subranges` entries.
    pub fn range_packet(&self, subranges: u16) -> Result<SetupPacket, ControlError> {
        Ok(SetupPacket::get_range(
            self.address,
            ParamLayout::Two.range_length(subranges)?,
        ))
    }

    /// SET CUR packet and its payload for `raw` (1/256 dB).
    pub fn set_request(&self, raw: i16) -> (SetupPacket, Vec<u8>) {
        let payload = encode_cur(ParamLayout::Two, u32::from(raw as u16));
        (
            SetupPacket::set_cur(self.address, ParamLayout::Two.cur_length()),
            payload,
        )
    }

    /// GET CUR read-back packet.
    pub fn readback_packet(&self) -> SetupPacket {
        SetupPacket::get_cur(self.address, ParamLayout::Two.cur_length())
    }
}

/// Whether the device declares any mute control.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MuteStatus {
    /// No Feature Unit declares a mute control.
    NoDeclaredMuteControl,
    /// `(feature unit id, channel)` pairs that declare mute.
    Declared(Vec<(u8, u8)>),
}

/// Inspect the descriptor for declared mute controls.
pub fn mute_status(ac: &AudioControl) -> MuteStatus {
    match ac.declared_mute_controls() {
        declared if declared.is_empty() => MuteStatus::NoDeclaredMuteControl,
        declared => MuteStatus::Declared(declared),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::find_control_interface;
    use crate::fixtures::{self, mk2_synthetic_interfaces};
    use crate::uac2::RequestAttribute;

    fn mk2() -> (AudioControl, ControlInterface) {
        let ifaces = mk2_synthetic_interfaces();
        (
            AudioControl::from_interfaces(&ifaces).unwrap(),
            find_control_interface(&ifaces).unwrap(),
        )
    }

    #[test]
    fn dump_plan_covers_the_four_targets_and_nothing_else() {
        let (ac, ci) = mk2();
        let plan = dump_plan(&ac, ci);
        assert!(plan.skipped.is_empty());
        assert!(plan
            .reads
            .iter()
            .any(|r| matches!(r.target, DumpTarget::ClockSourceSamFreq { .. })));
        assert!(plan
            .reads
            .iter()
            .any(|r| matches!(r.target, DumpTarget::ClockSelector { .. })));
        let fu_reads = plan
            .reads
            .iter()
            .filter(|r| matches!(r.target, DumpTarget::FeatureUnit { .. }))
            .count();
        assert_eq!(fu_reads, 8 + 4);
        let mixer_reads = plan
            .reads
            .iter()
            .filter(|r| matches!(r.target, DumpTarget::MixerNode { .. }))
            .count();
        assert_eq!(mixer_reads, 16 * 6);
    }

    #[test]
    fn dump_plan_is_get_only_and_never_addresses_extension_units() {
        let (ac, ci) = mk2();
        let xu_ids: Vec<u8> = ac.extension_units().map(|x| x.id).collect();
        assert_eq!(xu_ids.len(), 6);
        let plan = dump_plan(&ac, ci);
        for read in &plan.reads {
            for packet in read.known_packets() {
                assert!(packet.attribute.is_read());
                assert_ne!(packet.attribute, RequestAttribute::SetCur);
                assert!(!xu_ids.contains(&packet.address.entity_id));
                assert_eq!(packet.address.interface_number, fixtures::MK2_DFU_INTERFACE);
            }
        }
    }

    #[test]
    fn mixer_nodes_use_uac2_mixer_control_numbers() {
        let (ac, ci) = mk2();
        let plan = dump_plan(&ac, ci);
        let node = |u: u16, v: u8| {
            plan.reads
                .iter()
                .find(|r| {
                    matches!(r.target, DumpTarget::MixerNode { input, output, .. }
                        if input == u && output == v)
                })
                .copied()
                .unwrap()
        };
        // MCN = (u - 1) * m + (v - 1), m = 6
        assert_eq!(node(1, 1).address.w_value(), 0x0100);
        assert_eq!(node(2, 3).address.w_value(), 0x0108);
        assert_eq!(node(16, 6).address.w_value(), 0x015f);
        assert!(matches!(
            node(16, 6).target,
            DumpTarget::MixerNode {
                programmable: false,
                ..
            }
        ));
        assert_eq!(
            node(1, 1).cur_packet().to_bytes(),
            [0xa1, 0x01, 0x00, 0x01, 0x04, 0x3c, 0x02, 0x00]
        );
    }

    #[test]
    fn volume_targets_the_mixer_output_feature_unit() {
        let (ac, ci) = mk2();
        let target = volume_target(&ac, ci, 1).unwrap();
        assert_eq!(target.unit, fixtures::MK2_VOLUME_FEATURE_UNIT);
        let (set, payload) = target.set_request(-20 * 256);
        assert_eq!(
            set.to_bytes(),
            [0x21, 0x01, 0x01, 0x02, 0x04, 0x0c, 0x02, 0x00]
        );
        assert_eq!(payload, vec![0x00, 0xec]);
        assert_eq!(
            target.readback_packet().to_bytes(),
            [0xa1, 0x01, 0x01, 0x02, 0x04, 0x0c, 0x02, 0x00]
        );
        assert_eq!(
            target.range_count_packet().to_bytes(),
            [0xa1, 0x02, 0x01, 0x02, 0x04, 0x0c, 0x02, 0x00]
        );
        assert_eq!(target.range_packet(1).unwrap().w_length, 8);
    }

    #[test]
    fn volume_on_undeclared_channel_is_refused() {
        let (ac, ci) = mk2();
        assert_eq!(
            volume_target(&ac, ci, 5),
            Err(ControlError::VolumeNotDeclared {
                unit: fixtures::MK2_VOLUME_FEATURE_UNIT,
                channel: 5
            })
        );
        assert!(volume_target(&ac, ci, 0).is_err());
    }

    #[test]
    fn mk2_declares_no_mute_control() {
        let (ac, _) = mk2();
        assert_eq!(mute_status(&ac), MuteStatus::NoDeclaredMuteControl);
    }

    #[test]
    fn values_render_by_kind() {
        assert_eq!(
            format_value(ValueKind::Hertz, None, 48_000, &[]),
            "48000 Hz"
        );
        assert_eq!(
            format_value(ValueKind::Decibel256, None, 0xEC00, &[]),
            "-20.00 dB"
        );
        assert_eq!(
            format_value(ValueKind::DecibelQuarter, None, 0xFC, &[]),
            "-1.00 dB"
        );
        assert_eq!(format_value(ValueKind::Bool, None, 1, &[]), "true");
        assert_eq!(format_value(ValueKind::Raw, None, 0, &[1, 0xab]), "01 ab");
    }
}
