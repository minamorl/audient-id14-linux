//! Operations behind `dump`, `volume` and `mute`, written against the
//! injected [`ControlTransport`], plus the `--dry-run` renderers (which take
//! no transport and therefore cannot send anything).

use std::io::Write;

use id14_protocol::plan::{
    format_value, mute_status, DumpPlan, DumpRead, MuteStatus, VolumeTarget,
};
use id14_protocol::uac2::{parse_cur, parse_range, parse_range_count, ParamLayout, RangeBlock};
use id14_protocol::{AudioControl, SetupPacket};

use crate::error::CliError;
use crate::transport::ControlTransport;

/// Lowercase hex bytes separated by single spaces.
pub fn format_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One dry-run line: the 8 setup bytes, the payload, and the header row's
/// evidence status.
pub fn render_request(setup: &SetupPacket, payload: Option<&[u8]>) -> String {
    let payload = match payload {
        Some(bytes) if !bytes.is_empty() => format_bytes(bytes),
        _ => "none".to_string(),
    };
    format!(
        "{:<9}  setup: {}  payload: {}  [header: {}]",
        setup.attribute.label(),
        format_bytes(&setup.to_bytes()),
        payload,
        setup.attribute.header_evidence()
    )
}

fn render_range_followup(first: &SetupPacket, layout: ParamLayout) -> String {
    let bytes = first.to_bytes();
    format!(
        "{:<9}  setup: {} ?? ??  payload: none  (wLength = 2 + {} * n, n = wNumSubRanges from the reply above)",
        first.attribute.label(),
        format_bytes(&bytes[..6]),
        3 * layout.size()
    )
}

/// Print every request `dump` would send, sending nothing.
pub fn dry_run_dump(plan: &DumpPlan, out: &mut dyn Write) -> Result<(), CliError> {
    writeln!(
        out,
        "dry-run: nothing is sent; dump would address control interface {} (DFU)",
        plan.control_interface
    )?;
    for read in &plan.reads {
        writeln!(out, "{}", read.label())?;
        if let (Some(first), Some(layout)) = (read.range_count_packet(), read.layout) {
            writeln!(out, "  {}", render_request(&first, None))?;
            writeln!(out, "  {}", render_range_followup(&first, layout))?;
        }
        writeln!(out, "  {}", render_request(&read.cur_packet(), None))?;
    }
    for note in &plan.skipped {
        writeln!(out, "skipped: {note}")?;
    }
    Ok(())
}

/// Read a RANGE block in two steps: `wNumSubRanges`, then the full block.
pub fn read_range(
    transport: &mut dyn ControlTransport,
    first: &SetupPacket,
    layout: ParamLayout,
) -> Result<RangeBlock, CliError> {
    let head = transport.control_in(first)?;
    let count = parse_range_count(&head)?;
    let full = SetupPacket::get_range(first.address, layout.range_length(count)?);
    let bytes = transport.control_in(&full)?;
    Ok(parse_range(layout, &bytes)?)
}

fn format_range(read: &DumpRead, block: &RangeBlock) -> String {
    block
        .subranges
        .iter()
        .map(|s| {
            let show = |raw: u32| format_value(read.kind, read.layout, raw, &[]);
            format!("{}..{} step {}", show(s.min), show(s.max), show(s.res))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn dump_one(transport: &mut dyn ControlTransport, read: &DumpRead) -> Result<String, CliError> {
    let range = match (read.range_count_packet(), read.layout) {
        (Some(first), Some(layout)) => Some(read_range(transport, &first, layout)?),
        _ => None,
    };
    let bytes = transport.control_in(&read.cur_packet())?;
    let raw = match read.layout {
        Some(layout) => parse_cur(layout, &bytes)?,
        None => 0,
    };
    let mut line = format!(
        "{}: {}",
        read.label(),
        format_value(read.kind, read.layout, raw, &bytes)
    );
    if let Some(block) = range {
        line.push_str(&format!("  (range {})", format_range(read, &block)));
    }
    Ok(line)
}

/// Execute the read-only dump. Each failed read is reported in place; the
/// result is an error if any read failed.
pub fn run_dump(
    plan: &DumpPlan,
    transport: &mut dyn ControlTransport,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let mut failed = 0;
    for read in &plan.reads {
        match dump_one(transport, read) {
            Ok(line) => writeln!(out, "{line}")?,
            Err(err) => {
                failed += 1;
                writeln!(out, "{}: error: {err}", read.label())?;
            }
        }
    }
    for note in &plan.skipped {
        writeln!(out, "skipped: {note}")?;
    }
    match failed {
        0 => Ok(()),
        failed => Err(CliError::DumpIncomplete { failed }),
    }
}

/// Print every request `volume` would send, sending nothing. The range check
/// needs the device's reply, so it is not performed here.
pub fn dry_run_volume(
    target: &VolumeTarget,
    raw: i16,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    writeln!(
        out,
        "dry-run: nothing is sent; volume would address feature unit {:#04x} ch{} via control interface {} (DFU)",
        target.unit, target.channel, target.address.interface_number
    )?;
    let first = target.range_count_packet();
    writeln!(out, "  {}", render_request(&first, None))?;
    writeln!(out, "  {}", render_range_followup(&first, ParamLayout::Two))?;
    writeln!(
        out,
        "  (SET CUR is sent only if {} lies inside the reported range)",
        id14_protocol::db::format_raw_db(raw)
    )?;
    let (set, payload) = target.set_request(raw);
    writeln!(out, "  {}", render_request(&set, Some(&payload)))?;
    writeln!(out, "  {}", render_request(&target.readback_packet(), None))?;
    Ok(())
}

/// GET RANGE, reject out-of-range values without sending, SET CUR, then
/// GET CUR and display the read-back value. Returns the read-back raw value.
pub fn run_volume(
    target: &VolumeTarget,
    raw: i16,
    transport: &mut dyn ControlTransport,
    out: &mut dyn Write,
) -> Result<i16, CliError> {
    let range = read_range(transport, &target.range_count_packet(), ParamLayout::Two)?;
    range.check_signed(i64::from(raw))?;
    let (set, payload) = target.set_request(raw);
    transport.control_out(&set, &payload)?;
    let bytes = transport.control_in(&target.readback_packet())?;
    let readback = parse_cur(ParamLayout::Two, &bytes)? as u16 as i16;
    writeln!(
        out,
        "feature unit {:#04x} ch{} VOLUME: set {} -> read back {}",
        target.unit,
        target.channel,
        id14_protocol::db::format_raw_db(raw),
        id14_protocol::db::format_raw_db(readback)
    )?;
    Ok(readback)
}

/// `mute` never writes: with no declared mute control it fails saying so,
/// and it never substitutes a minimum volume.
pub fn mute_error(ac: &AudioControl) -> CliError {
    match mute_status(ac) {
        MuteStatus::NoDeclaredMuteControl => CliError::NoDeclaredMuteControl,
        MuteStatus::Declared(declared) => CliError::MuteNotSpecified(declared),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportError;
    use id14_protocol::descriptor::find_control_interface;
    use id14_protocol::fixtures::{self, mk2_synthetic_interfaces};
    use id14_protocol::plan::{dump_plan, volume_target};
    use id14_protocol::{ControlError, ControlInterface, RequestAttribute};

    /// Scripted fake device: answers GET requests from a closure and records
    /// every request it receives.
    struct FakeDevice<F: FnMut(&SetupPacket) -> Option<Vec<u8>>> {
        answer: F,
        sent: Vec<(SetupPacket, Vec<u8>)>,
    }

    impl<F: FnMut(&SetupPacket) -> Option<Vec<u8>>> ControlTransport for FakeDevice<F> {
        fn control_in(&mut self, setup: &SetupPacket) -> Result<Vec<u8>, TransportError> {
            self.sent.push((*setup, Vec::new()));
            (self.answer)(setup).ok_or_else(|| TransportError {
                setup: *setup,
                cause: "stall".into(),
            })
        }

        fn control_out(
            &mut self,
            setup: &SetupPacket,
            payload: &[u8],
        ) -> Result<(), TransportError> {
            self.sent.push((*setup, payload.to_vec()));
            Ok(())
        }
    }

    fn mk2() -> (AudioControl, ControlInterface) {
        let ifaces = mk2_synthetic_interfaces();
        (
            AudioControl::from_interfaces(&ifaces).unwrap(),
            find_control_interface(&ifaces).unwrap(),
        )
    }

    /// Volume RANGE of the pinned mk2 FU 0x0C: -127 dB .. 0 dB step 1 dB.
    fn volume_range_reply(setup: &SetupPacket) -> Vec<u8> {
        let mut block = vec![1, 0];
        block.extend_from_slice(&(-127i16 * 256).to_le_bytes());
        block.extend_from_slice(&0i16.to_le_bytes());
        block.extend_from_slice(&256i16.to_le_bytes());
        block.truncate(usize::from(setup.w_length));
        block
    }

    fn mk2_answers(setup: &SetupPacket) -> Option<Vec<u8>> {
        let len = usize::from(setup.w_length);
        match setup.attribute {
            RequestAttribute::GetRange if setup.address.control_selector == 0x02 => {
                Some(volume_range_reply(setup))
            }
            RequestAttribute::GetRange => {
                // SAM_FREQ: 44100..48000, one subrange
                let mut block = vec![1, 0];
                for v in [44_100u32, 48_000, 0] {
                    block.extend_from_slice(&v.to_le_bytes());
                }
                block.truncate(len);
                Some(block)
            }
            RequestAttribute::GetCur => Some(vec![0; len]),
            RequestAttribute::SetCur => None,
        }
    }

    #[test]
    fn format_bytes_is_exact_lowercase_hex() {
        assert_eq!(format_bytes(&[0xa1, 0x01, 0x0c, 0xff]), "a1 01 0c ff");
        assert_eq!(format_bytes(&[]), "");
    }

    #[test]
    fn dry_run_line_shows_setup_payload_and_evidence() {
        let (ac, ci) = mk2();
        let target = volume_target(&ac, ci, 1).unwrap();
        let (set, payload) = target.set_request(-20 * 256);
        assert_eq!(
            render_request(&set, Some(&payload)),
            "SET CUR    setup: 21 01 01 02 04 0c 02 00  payload: 00 ec  [header: static_analysis_inferred_unverified]"
        );
        assert_eq!(
            render_request(&target.readback_packet(), None),
            "GET CUR    setup: a1 01 01 02 04 0c 02 00  payload: none  [header: mk2_hardware_observed_2026-09-24]"
        );
    }

    #[test]
    fn dump_sends_only_get_requests_and_never_touches_extension_units() {
        let (ac, ci) = mk2();
        let plan = dump_plan(&ac, ci);
        let mut device = FakeDevice {
            answer: mk2_answers,
            sent: Vec::new(),
        };
        let mut out = Vec::new();
        run_dump(&plan, &mut device, &mut out).unwrap();
        let xu_ids: Vec<u8> = ac.extension_units().map(|x| x.id).collect();
        assert!(!device.sent.is_empty());
        for (setup, payload) in &device.sent {
            assert!(setup.attribute.is_read(), "dump sent {:?}", setup);
            assert!(payload.is_empty());
            assert!(!xu_ids.contains(&setup.address.entity_id));
            assert_eq!(setup.address.interface_number, fixtures::MK2_DFU_INTERFACE);
        }
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("SAM_FREQ: 0 Hz  (range 44100 Hz..48000 Hz step 0 Hz)"));
        assert!(text.contains(
            "feature unit 0x0c ch1 VOLUME: 0.00 dB  (range -127.00 dB..0.00 dB step 1.00 dB)"
        ));
        assert!(text.contains("mixer unit 0x3c in16->out6 (MCN 95) [fixed]: 0.00 dB"));
    }

    #[test]
    fn dump_reports_failed_reads_and_continues() {
        let (ac, ci) = mk2();
        let plan = dump_plan(&ac, ci);
        let mut device = FakeDevice {
            answer: |s: &SetupPacket| {
                (s.address.entity_id != fixtures::SYNTH_CLOCK_SELECTOR).then(|| mk2_answers(s))?
            },
            sent: Vec::new(),
        };
        let mut out = Vec::new();
        let result = run_dump(&plan, &mut device, &mut out);
        assert!(matches!(
            result,
            Err(CliError::DumpIncomplete { failed: 1 })
        ));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("clock selector 0x28 SELECTOR: error:"));
        assert!(text.contains("mixer unit 0x3c in1->out1"));
    }

    #[test]
    fn volume_in_range_sets_then_reads_back() {
        let (ac, ci) = mk2();
        let target = volume_target(&ac, ci, 2).unwrap();
        let mut device = FakeDevice {
            answer: |s: &SetupPacket| match s.attribute {
                RequestAttribute::GetRange => Some(volume_range_reply(s)),
                RequestAttribute::GetCur => Some((-20i16 * 256).to_le_bytes().to_vec()),
                RequestAttribute::SetCur => None,
            },
            sent: Vec::new(),
        };
        let mut out = Vec::new();
        let readback = run_volume(&target, -20 * 256, &mut device, &mut out).unwrap();
        assert_eq!(readback, -20 * 256);
        let kinds: Vec<RequestAttribute> = device.sent.iter().map(|(s, _)| s.attribute).collect();
        assert_eq!(
            kinds,
            vec![
                RequestAttribute::GetRange,
                RequestAttribute::GetRange,
                RequestAttribute::SetCur,
                RequestAttribute::GetCur
            ]
        );
        let (set, payload) = &device.sent[2];
        assert_eq!(set.address.entity_id, fixtures::MK2_VOLUME_FEATURE_UNIT);
        assert_eq!(set.address.channel, 2);
        assert_eq!(payload, &vec![0x00, 0xec]);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("set -20.00 dB -> read back -20.00 dB"));
    }

    #[test]
    fn volume_out_of_range_is_rejected_without_set() {
        let (ac, ci) = mk2();
        let target = volume_target(&ac, ci, 1).unwrap();
        let mut device = FakeDevice {
            answer: |s: &SetupPacket| Some(volume_range_reply(s)),
            sent: Vec::new(),
        };
        let mut out = Vec::new();
        let result = run_volume(&target, 3 * 256, &mut device, &mut out);
        assert!(matches!(
            result,
            Err(CliError::Control(ControlError::OutOfRange {
                requested: 768,
                ..
            }))
        ));
        assert!(device
            .sent
            .iter()
            .all(|(s, _)| s.attribute != RequestAttribute::SetCur));
        assert!(out.is_empty());
    }

    #[test]
    fn dry_run_volume_prints_set_bytes_and_payload() {
        let (ac, ci) = mk2();
        let target = volume_target(&ac, ci, 1).unwrap();
        let mut out = Vec::new();
        dry_run_volume(&target, -20 * 256, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("setup: a1 02 01 02 04 0c 02 00"));
        assert!(text.contains("setup: 21 01 01 02 04 0c 02 00  payload: 00 ec"));
        assert!(text.contains("setup: a1 01 01 02 04 0c 02 00"));
    }

    #[test]
    fn dry_run_dump_lists_requests() {
        let (ac, ci) = mk2();
        let mut out = Vec::new();
        dry_run_dump(&dump_plan(&ac, ci), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("control interface 4 (DFU)"));
        assert!(!text.contains("SET CUR"));
        // clock source SAM_FREQ: GET RANGE (count) then GET CUR
        assert!(text.contains("setup: a1 02 00 01 04 29 02 00"));
        assert!(text.contains("setup: a1 01 00 01 04 29 04 00"));
    }

    #[test]
    fn mute_fails_stating_no_declared_mute_control() {
        let (ac, _) = mk2();
        let err = mute_error(&ac);
        assert!(matches!(err, CliError::NoDeclaredMuteControl));
        assert!(err.to_string().contains("declares no mute control"));
    }
}
