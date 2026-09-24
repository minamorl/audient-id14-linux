//! `id14ctl` — command-line control tool for the Audient iD14 (mk2 primary).

use std::error::Error;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use id14_protocol::db::parse_db;
use id14_protocol::descriptor::find_control_interface;
use id14_protocol::plan::{dump_plan, volume_target};
use id14_protocol::{AudioControl, ControlRequest, RequestKind, Variant};
use id14ctl::ops::{dry_run_dump, dry_run_volume, format_bytes, mute_error, run_dump, run_volume};
use id14ctl::usb::{enumerate, select_primary, DetectedDevice};
use id14ctl::CliError;

#[derive(Debug, Parser)]
#[command(
    name = "id14ctl",
    version,
    about = "Unofficial control tool for the Audient iD14 (Linux)"
)]
struct Cli {
    /// Print the exact bytes that would be sent (8-byte setup packet and
    /// payload) and perform no USB transfer.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Allow write operations (volume, mute, SET requests). Off by default.
    #[arg(long, global = true)]
    enable_write: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Enumerate connected iD14 devices (mk2 first).
    List,
    /// Identify the auto-detected device's product and PID.
    Info,
    /// Read-only: GET the descriptor-declared standard controls and print them.
    Dump,
    /// Print the bytes of a static-analysis framed request (--dry-run only).
    Request {
        /// Request kind (selects the pinned header).
        #[arg(value_enum)]
        kind: RequestKindArg,
        /// Bytes following the header (hex like 0x0c or decimal).
        #[arg(value_parser = parse_byte)]
        body: Vec<u8>,
    },
    /// SET CUR the volume of the mixer-output Feature Unit (requires --enable-write).
    Volume {
        /// Level in dB, e.g. -20 or -20.5.
        #[arg(allow_negative_numbers = true)]
        db: String,
        /// Channel number of the Feature Unit (0 = master).
        #[arg(long)]
        channel: u8,
    },
    /// Mute (requires --enable-write; fails when no mute control is declared).
    Mute,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RequestKindArg {
    Get,
    Set,
    GetMem,
}

impl From<RequestKindArg> for RequestKind {
    fn from(value: RequestKindArg) -> Self {
        match value {
            RequestKindArg::Get => RequestKind::Get,
            RequestKindArg::Set => RequestKind::Set,
            RequestKindArg::GetMem => RequestKind::GetMem,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match run(cli, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = out.flush();
            eprintln!("error: {err}");
            let mut cause = err.source();
            while let Some(inner) = cause {
                eprintln!("  caused by: {inner}");
                cause = inner.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli, out: &mut dyn Write) -> Result<(), CliError> {
    match cli.command {
        Command::List => list_devices(out),
        Command::Info => show_info(out),
        Command::Dump => dump(cli.dry_run, out),
        Command::Request { kind, body } => request(kind, body, cli.dry_run, cli.enable_write, out),
        Command::Volume { db, channel } => {
            require_write_enabled(cli.enable_write)?;
            volume(&db, channel, cli.dry_run, out)
        }
        Command::Mute => {
            require_write_enabled(cli.enable_write)?;
            let device = select_primary()?;
            let ac = AudioControl::from_interfaces(&device.interfaces()?)?;
            Err(mute_error(&ac))
        }
    }
}

fn describe(device: &DetectedDevice) -> String {
    format!(
        "bus {:03} address {:03}  {:04x}:{:04x}  {}",
        device.bus, device.address, device.product.vid, device.product.pid, device.product.name
    )
}

fn list_devices(out: &mut dyn Write) -> Result<(), CliError> {
    let devices = enumerate()?;
    if devices.is_empty() {
        writeln!(out, "no supported Audient iD14 device found")?;
    }
    for device in &devices {
        writeln!(out, "{}", describe(device))?;
    }
    Ok(())
}

fn evidence_note(variant: Variant) -> &'static str {
    match variant {
        Variant::Mk2 => "mk2 (primary target)",
        Variant::Mk1 => "mk1 (static-analysis inferred only, not verified on hardware)",
    }
}

fn show_info(out: &mut dyn Write) -> Result<(), CliError> {
    let device = select_primary()?;
    writeln!(out, "product: {}", device.product.name)?;
    writeln!(out, "variant: {}", evidence_note(device.product.variant))?;
    writeln!(
        out,
        "vid:pid: {:04x}:{:04x}",
        device.product.vid, device.product.pid
    )?;
    writeln!(
        out,
        "usb:     bus {:03} address {:03}",
        device.bus, device.address
    )?;
    Ok(())
}

fn dump(dry_run: bool, out: &mut dyn Write) -> Result<(), CliError> {
    let device = select_primary()?;
    let interfaces = device.interfaces()?;
    let ac = AudioControl::from_interfaces(&interfaces)?;
    let plan = dump_plan(&ac, find_control_interface(&interfaces)?);
    if dry_run {
        return dry_run_dump(&plan, out);
    }
    let mut transport = device.open_control(&interfaces)?;
    run_dump(&plan, &mut transport, out)
}

fn volume(db: &str, channel: u8, dry_run: bool, out: &mut dyn Write) -> Result<(), CliError> {
    let raw = parse_db(db)?;
    let device = select_primary()?;
    let interfaces = device.interfaces()?;
    let ac = AudioControl::from_interfaces(&interfaces)?;
    let target = volume_target(&ac, find_control_interface(&interfaces)?, channel)?;
    if dry_run {
        return dry_run_volume(&target, raw, out);
    }
    let mut transport = device.open_control(&interfaces)?;
    run_volume(&target, raw, &mut transport, out).map(|_| ())
}

fn request(
    kind: RequestKindArg,
    body: Vec<u8>,
    dry_run: bool,
    enable_write: bool,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    if matches!(kind, RequestKindArg::Set) {
        require_write_enabled(enable_write)?;
    }
    if !dry_run {
        return Err(CliError::RequestSendUnsupported);
    }
    let request = ControlRequest::new(kind.into(), body);
    writeln!(out, "{}", format_bytes(&request.to_bytes()))?;
    Ok(())
}

fn require_write_enabled(enable_write: bool) -> Result<(), CliError> {
    if enable_write {
        Ok(())
    } else {
        Err(CliError::WriteDisabled)
    }
}

fn parse_byte(input: &str) -> Result<u8, String> {
    let parsed = match input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        Some(hex) => u8::from_str_radix(hex, 16),
        None => input.parse::<u8>(),
    };
    parsed.map_err(|e| format!("invalid byte {input:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use id14_protocol::{ProductDefinition, PID_MK1, PID_MK2, VID};

    #[test]
    fn clap_definition_and_representative_arguments_are_valid() {
        Cli::command().debug_assert();
        for args in [
            vec!["id14ctl", "list"],
            vec!["id14ctl", "info"],
            vec!["id14ctl", "dump", "--dry-run"],
            vec!["id14ctl", "--dry-run", "request", "get", "0x01", "2"],
            vec![
                "id14ctl",
                "--enable-write",
                "volume",
                "-20",
                "--channel",
                "1",
            ],
            vec!["id14ctl", "volume", "--dry-run", "-20.5", "--channel", "0"],
            vec!["id14ctl", "--enable-write", "mute"],
        ] {
            Cli::try_parse_from(&args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
        }
        assert!(Cli::try_parse_from(["id14ctl", "volume", "-20"]).is_err());
    }

    #[test]
    fn request_bytes_are_deterministic_for_all_pinned_headers() {
        for (kind, header) in [
            (RequestKindArg::Get, "a1 01"),
            (RequestKindArg::Set, "21 01"),
            (RequestKindArg::GetMem, "a1 03"),
        ] {
            let mut out = Vec::new();
            request(kind, vec![0x0c, 0x02], true, true, &mut out).unwrap();
            assert_eq!(String::from_utf8(out).unwrap(), format!("{header} 0c 02\n"));
        }
    }

    #[test]
    fn protocol_lookup_identifies_mk1_and_mk2_vid_pid_pairs() {
        assert_eq!(
            ProductDefinition::lookup(VID, PID_MK2).unwrap().variant,
            Variant::Mk2
        );
        assert_eq!(
            ProductDefinition::lookup(VID, PID_MK1).unwrap().variant,
            Variant::Mk1
        );
        assert!(ProductDefinition::lookup(VID, 0xffff).is_err());
        assert!(
            id14ctl::usb::autodetect_rank(Variant::Mk2)
                < id14ctl::usb::autodetect_rank(Variant::Mk1)
        );
        assert!(evidence_note(Variant::Mk1).contains("not verified on hardware"));
    }

    #[test]
    fn write_commands_are_disabled_without_explicit_gate() {
        assert!(matches!(
            require_write_enabled(false),
            Err(CliError::WriteDisabled)
        ));
        assert!(require_write_enabled(true).is_ok());
        for args in [
            vec!["id14ctl", "volume", "-20", "--channel", "1"],
            vec!["id14ctl", "--dry-run", "volume", "-20", "--channel", "1"],
            vec!["id14ctl", "mute"],
            vec!["id14ctl", "--dry-run", "request", "set", "0x01"],
        ] {
            let cli = Cli::try_parse_from(&args).unwrap();
            let mut out = Vec::new();
            assert!(
                matches!(run(cli, &mut out), Err(CliError::WriteDisabled)),
                "{args:?} was not gated"
            );
            assert!(out.is_empty());
        }
    }

    #[test]
    fn dry_run_format_is_exact_lowercase_hex() {
        let mut out = Vec::new();
        request(RequestKindArg::Get, vec![0xAB, 0x0C], true, false, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "a1 01 ab 0c\n");
        assert!(matches!(
            request(RequestKindArg::Get, vec![], false, false, &mut Vec::new()),
            Err(CliError::RequestSendUnsupported)
        ));
        assert_eq!(parse_byte("0x0C"), Ok(0x0c));
        assert_eq!(parse_byte("12"), Ok(12));
        assert!(parse_byte("0x100").is_err());
    }
}
