//! Test fixture shaped like the iD14 MKII (PID `0x0008`).
//!
//! **This is not a descriptor capture.** It is assembled from the values the
//! spec pins for mk2 — AC protocol 0x20, HID interface 3, DFU interface 4,
//! mixer unit 0x3C (16 in x 6 out), mixer-output Feature Unit 0x0C with volume
//! on channels 1-4, Feature Unit 0x0B, Extension Units 0x36 (code 1) and 0x37
//! (code 2), Extension Units 0x3E / 0x32 / 0x34 / 0x33 (code 0), and no mute
//! control anywhere. Every other id and field (clock ids, terminal ids,
//! channel splits, wiring) is synthetic and chosen only to make the topology
//! consistent. Run-time code never reads these constants; entity ids always
//! come from the connected device's descriptor.

use crate::descriptor::{InterfaceInfo, CLASS_AUDIO, CLASS_DFU, CS_INTERFACE, PROTOCOL_UAC2};

/// DFU interface number on mk2 (pinned).
pub const MK2_DFU_INTERFACE: u8 = 4;
/// Mixer-output Feature Unit id on mk2 (pinned).
pub const MK2_VOLUME_FEATURE_UNIT: u8 = 0x0C;
/// Per-channel Feature Unit id on mk2 (pinned).
pub const MK2_CHANNEL_FEATURE_UNIT: u8 = 0x0B;
/// Mixer unit id on mk2 (pinned).
pub const MK2_MIXER_UNIT: u8 = 0x3C;
/// Extension unit carrying extension code 1 (MonitorController) on mk2.
pub const MK2_MONITOR_CONTROLLER_XU: u8 = 0x36;
/// Extension unit carrying extension code 2 (MonoMixController) on mk2.
pub const MK2_MONO_MIX_CONTROLLER_XU: u8 = 0x37;
/// Extension units carrying extension code 0 on mk2.
pub const MK2_CODE_ZERO_XUS: [u8; 4] = [0x3E, 0x32, 0x34, 0x33];
/// Synthetic clock source id.
pub const SYNTH_CLOCK_SOURCE: u8 = 0x29;
/// Synthetic clock selector id.
pub const SYNTH_CLOCK_SELECTOR: u8 = 0x28;

const VOLUME_PROGRAMMABLE: u32 = 0b11 << 2;

fn input_terminal(id: u8, terminal_type: u16, channels: u8) -> Vec<u8> {
    let t = terminal_type.to_le_bytes();
    vec![
        17,
        CS_INTERFACE,
        0x02,
        id,
        t[0],
        t[1],
        0,
        SYNTH_CLOCK_SELECTOR,
        channels,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ]
}

fn feature_unit(id: u8, source: u8, per_channel: &[u32]) -> Vec<u8> {
    let mut d = vec![0, CS_INTERFACE, 0x06, id, source];
    for bitmap in per_channel {
        d.extend_from_slice(&bitmap.to_le_bytes());
    }
    d.push(0);
    d[0] = d.len() as u8;
    d
}

fn extension_unit(id: u8, code: u16, source: u8, channels: u8) -> Vec<u8> {
    let c = code.to_le_bytes();
    vec![
        16,
        CS_INTERFACE,
        0x09,
        id,
        c[0],
        c[1],
        1,
        source,
        channels,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ]
}

fn mixer_unit(id: u8, sources: &[u8], outputs: u8, bitmap: &[u8]) -> Vec<u8> {
    let mut d = vec![0, CS_INTERFACE, 0x04, id, sources.len() as u8];
    d.extend_from_slice(sources);
    d.extend_from_slice(&[outputs, 0, 0, 0, 0, 0]);
    d.extend_from_slice(bitmap);
    d.extend_from_slice(&[0, 0]);
    d[0] = d.len() as u8;
    d
}

/// AudioControl class-specific bytes of the synthetic mk2 topology.
pub fn mk2_synthetic_audio_control_extra() -> Vec<u8> {
    let mut extra = Vec::new();
    // AC header, bcdADC 2.00
    extra.extend_from_slice(&[9, CS_INTERFACE, 0x01, 0x00, 0x02, 0x08, 0, 0, 0]);
    // clock source: internal programmable, frequency programmable, validity read-only
    extra.extend_from_slice(&[8, CS_INTERFACE, 0x0A, SYNTH_CLOCK_SOURCE, 0x03, 0x07, 0, 0]);
    // clock selector with one input pin, selector control programmable
    extra.extend_from_slice(&[
        8,
        CS_INTERFACE,
        0x0B,
        SYNTH_CLOCK_SELECTOR,
        1,
        SYNTH_CLOCK_SOURCE,
        0x03,
        0,
    ]);
    extra.extend(input_terminal(0x01, 0x0201, 8));
    extra.extend(input_terminal(0x02, 0x0101, 8));
    let mut fu0b = vec![0u32];
    fu0b.extend([VOLUME_PROGRAMMABLE; 8]);
    extra.extend(feature_unit(MK2_CHANNEL_FEATURE_UNIT, 0x01, &fu0b));
    extra.extend(extension_unit(MK2_MONITOR_CONTROLLER_XU, 1, 0x02, 8));
    // 16 x 6 = 96 nodes = 12 bytes; the last 8 nodes are non-programmable
    let mut bitmap = [0xFFu8; 12];
    bitmap[11] = 0x00;
    extra.extend(mixer_unit(
        MK2_MIXER_UNIT,
        &[MK2_CHANNEL_FEATURE_UNIT, MK2_MONITOR_CONTROLLER_XU],
        6,
        &bitmap,
    ));
    let fu0c = [
        0,
        VOLUME_PROGRAMMABLE,
        VOLUME_PROGRAMMABLE,
        VOLUME_PROGRAMMABLE,
        VOLUME_PROGRAMMABLE,
        0,
        0,
    ];
    extra.extend(feature_unit(MK2_VOLUME_FEATURE_UNIT, MK2_MIXER_UNIT, &fu0c));
    extra.extend(extension_unit(MK2_CODE_ZERO_XUS[0], 0, MK2_MIXER_UNIT, 6));
    extra.extend(extension_unit(
        MK2_MONO_MIX_CONTROLLER_XU,
        2,
        MK2_VOLUME_FEATURE_UNIT,
        6,
    ));
    extra.extend(extension_unit(MK2_CODE_ZERO_XUS[1], 0, 0x01, 8));
    extra.extend(extension_unit(MK2_CODE_ZERO_XUS[2], 0, 0x02, 8));
    extra.extend(extension_unit(
        MK2_CODE_ZERO_XUS[3],
        0,
        MK2_MONO_MIX_CONTROLLER_XU,
        6,
    ));
    // output terminal (speaker) fed by the last extension unit
    extra.extend_from_slice(&[
        12,
        CS_INTERFACE,
        0x03,
        0x14,
        0x01,
        0x03,
        0,
        MK2_CODE_ZERO_XUS[3],
        SYNTH_CLOCK_SELECTOR,
        0,
        0,
        0,
    ]);
    extra
}

/// Interfaces of the synthetic mk2 configuration: AC (0), two streaming (1-2),
/// HID (3), DFU (4).
pub fn mk2_synthetic_interfaces() -> Vec<InterfaceInfo> {
    let plain =
        |number: u8, alt_setting: u8, class: u8, subclass: u8, protocol: u8| InterfaceInfo {
            number,
            alt_setting,
            class,
            subclass,
            protocol,
            extra: Vec::new(),
        };
    let mut ac = plain(0, 0, CLASS_AUDIO, 0x01, PROTOCOL_UAC2);
    ac.extra = mk2_synthetic_audio_control_extra();
    vec![
        ac,
        plain(1, 0, CLASS_AUDIO, 0x02, PROTOCOL_UAC2),
        plain(1, 1, CLASS_AUDIO, 0x02, PROTOCOL_UAC2),
        plain(2, 0, CLASS_AUDIO, 0x02, PROTOCOL_UAC2),
        plain(2, 1, CLASS_AUDIO, 0x02, PROTOCOL_UAC2),
        plain(3, 0, 0x03, 0x00, 0x00),
        plain(MK2_DFU_INTERFACE, 0, CLASS_DFU, 0x01, 0x01),
    ]
}
