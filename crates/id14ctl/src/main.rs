use std::error::Error;
use std::fmt;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use id14_protocol::{
    ControlRequest, ProductDefinition, ProtocolError, RequestKind, PID_MK2, PRODUCTS, VID,
};

const TRANSPORT_UNIMPLEMENTED: &str =
    "not yet implemented: control transport undecided (see spec quarantine control.path) and no hardware verified";

#[derive(Debug, Parser)]
#[command(name = "id14ctl")]
#[command(about = "Minimal Linux CLI for Audient iD14 descriptor discovery and request dry-runs")]
struct Cli {
    /// Print request bytes without performing a USB transfer.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Explicitly enable commands that could write device state.
    #[arg(long, global = true)]
    enable_write: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Enumerate attached iD14 MKI and MKII USB descriptors.
    List,
    /// Identify the preferred attached iD14 (MKII before MKI).
    Info,
    /// Read device state without issuing any write/OUT transfer.
    Dump,
    /// Build a protocol request. A USB transfer is never attempted.
    Request {
        #[arg(value_enum)]
        kind: RequestKindArg,

        /// Body bytes, written as decimal or 0x-prefixed hexadecimal values.
        #[arg(value_parser = parse_byte)]
        body: Vec<u8>,
    },
    /// Set volume. Live transport is quarantined and therefore unavailable.
    Volume { value: u8 },
    /// Set mute state. Live transport is quarantined and therefore unavailable.
    Mute {
        #[arg(value_enum)]
        state: MuteState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum RequestKindArg {
    Get,
    Set,
    GetMem,
}

impl From<RequestKindArg> for RequestKind {
    fn from(value: RequestKindArg) -> Self {
        match value {
            RequestKindArg::Get => Self::Get,
            RequestKindArg::Set => Self::Set,
            RequestKindArg::GetMem => Self::GetMem,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum MuteState {
    On,
    Off,
}

#[derive(Debug)]
enum CliError {
    Usb {
        operation: &'static str,
        source: rusb::Error,
    },
    Protocol(ProtocolError),
    NoSupportedDevice,
    WriteDisabled,
    TransportUndecided,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usb { operation, source } => {
                write!(formatter, "{operation} failed: {source}")
            }
            Self::Protocol(source) => write!(formatter, "product lookup failed: {source}"),
            Self::NoSupportedDevice => write!(
                formatter,
                "no supported Audient iD14 device found (default target: mk2, PID 0x{PID_MK2:04x})"
            ),
            Self::WriteDisabled => write!(
                formatter,
                "write operation refused: pass --enable-write explicitly (writes are disabled by default)"
            ),
            Self::TransportUndecided => formatter.write_str(TRANSPORT_UNIMPLEMENTED),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Usb { source, .. } => Some(source),
            Self::Protocol(source) => Some(source),
            Self::NoSupportedDevice | Self::WriteDisabled | Self::TransportUndecided => None,
        }
    }
}

impl From<ProtocolError> for CliError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct DetectedDevice {
    bus: u8,
    address: u8,
    name: String,
    variant: &'static str,
    vid: u16,
    pid: u16,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::List => list_devices(),
        Command::Info => show_info(),
        Command::Dump => Err(CliError::TransportUndecided),
        Command::Request { kind, body } => {
            let request = build_request(kind, body);
            if cli.dry_run {
                println!("bytes: {}", format_bytes(&request.to_bytes()));
                Ok(())
            } else {
                if kind == RequestKindArg::Set && !cli.enable_write {
                    return Err(CliError::WriteDisabled);
                }
                Err(CliError::TransportUndecided)
            }
        }
        Command::Volume { value } => {
            let _requested_value = value;
            require_write_enabled(cli.enable_write)?;
            Err(CliError::TransportUndecided)
        }
        Command::Mute { state } => {
            let _requested_state = state;
            require_write_enabled(cli.enable_write)?;
            Err(CliError::TransportUndecided)
        }
    }
}

fn list_devices() -> Result<(), CliError> {
    let devices = enumerate_supported_devices()?;
    for device in devices {
        println!(
            "bus {:03} device {:03}: {} {} {:04x}:{:04x}",
            device.bus, device.address, device.name, device.variant, device.vid, device.pid
        );
    }
    Ok(())
}

fn show_info() -> Result<(), CliError> {
    let mut devices = enumerate_supported_devices()?;
    devices.sort_by_key(|device| if device.pid == PID_MK2 { 0 } else { 1 });
    let device = devices.first().ok_or(CliError::NoSupportedDevice)?;
    println!(
        "{} {} (VID 0x{:04x}, PID 0x{:04x}) at bus {:03} device {:03}",
        device.name, device.variant, device.vid, device.pid, device.bus, device.address
    );
    Ok(())
}

fn enumerate_supported_devices() -> Result<Vec<DetectedDevice>, CliError> {
    let usb_devices = rusb::devices().map_err(|source| CliError::Usb {
        operation: "USB descriptor enumeration",
        source,
    })?;
    let mut matches = Vec::new();

    for device in usb_devices.iter() {
        let descriptor = device.device_descriptor().map_err(|source| CliError::Usb {
            operation: "USB device descriptor read",
            source,
        })?;
        let vid = descriptor.vendor_id();
        let pid = descriptor.product_id();
        if vid != VID || !PRODUCTS.iter().any(|product| product.pid == pid) {
            continue;
        }

        let product = lookup_product(vid, pid)?;
        let variant = match product.variant {
            id14_protocol::Variant::Mk1 => "mk1",
            id14_protocol::Variant::Mk2 => "mk2",
        };
        matches.push(DetectedDevice {
            bus: device.bus_number(),
            address: device.address(),
            name: product.name.to_string(),
            variant,
            vid: product.vid,
            pid: product.pid,
        });
    }

    Ok(matches)
}

fn lookup_product(vid: u16, pid: u16) -> Result<ProductDefinition, CliError> {
    ProductDefinition::lookup(vid, pid).map_err(CliError::from)
}

fn build_request(kind: RequestKindArg, body: Vec<u8>) -> ControlRequest {
    ControlRequest::new(kind.into(), body)
}

fn require_write_enabled(enable_write: bool) -> Result<(), CliError> {
    if enable_write {
        Ok(())
    } else {
        Err(CliError::WriteDisabled)
    }
}

fn parse_byte(input: &str) -> Result<u8, String> {
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u8::from_str_radix(hex, 16).map_err(|error| format!("invalid byte {input:?}: {error}"))
    } else {
        input
            .parse::<u8>()
            .map_err(|error| format!("invalid byte {input:?}: {error}"))
    }
}

fn format_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use id14_protocol::{Variant, PID_MK1};

    #[test]
    fn clap_definition_and_representative_arguments_are_valid() {
        Cli::command().debug_assert();

        let parsed =
            Cli::try_parse_from(["id14ctl", "--dry-run", "request", "get-mem", "0x10", "32"]);
        assert!(parsed.is_ok());

        let parsed = Cli::try_parse_from(["id14ctl", "--enable-write", "mute", "on"]);
        assert!(parsed.is_ok());
    }

    #[test]
    fn request_bytes_are_deterministic_for_all_pinned_headers() {
        let get = build_request(RequestKindArg::Get, vec![0x10, 0x20]).to_bytes();
        let set = build_request(RequestKindArg::Set, vec![0x30]).to_bytes();
        let get_mem = build_request(RequestKindArg::GetMem, Vec::new()).to_bytes();

        assert_eq!(get, vec![0xa1, 0x01, 0x10, 0x20]);
        assert_eq!(set, vec![0x21, 0x01, 0x30]);
        assert_eq!(get_mem, vec![0xa1, 0x03]);
    }

    #[test]
    fn protocol_lookup_identifies_mk1_and_mk2_vid_pid_pairs() {
        let mk1 = lookup_product(VID, PID_MK1);
        let mk2 = lookup_product(VID, PID_MK2);

        assert!(
            matches!(mk1, Ok(product) if product.variant == Variant::Mk1 && product.pid == PID_MK1)
        );
        assert!(
            matches!(mk2, Ok(product) if product.variant == Variant::Mk2 && product.pid == PID_MK2)
        );
    }

    #[test]
    fn write_commands_are_disabled_without_explicit_gate() {
        assert!(matches!(
            require_write_enabled(false),
            Err(CliError::WriteDisabled)
        ));
        assert!(require_write_enabled(true).is_ok());
    }

    #[test]
    fn dry_run_format_is_exact_lowercase_hex() {
        assert_eq!(format_bytes(&[0xa1, 0x01, 0xff]), "a1 01 ff");
    }
}
