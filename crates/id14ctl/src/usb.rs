//! libusb (rusb) adapter: enumeration, descriptor model, and the live
//! control transport.
//!
//! Reading descriptors uses libusb's cached copy and sends no control
//! request. The live transport claims only the DFU interface, never
//! interfaces 0-2, and never detaches a kernel driver (auto-detach stays at
//! libusb's default, off), so snd-usb-audio keeps the audio interfaces.

use std::time::Duration;

use id14_protocol::descriptor::find_control_interface;
use id14_protocol::{
    ControlError, ControlInterface, InterfaceInfo, ProductDefinition, SetupPacket, Variant, VID,
};
use rusb::{ConfigDescriptor, Device, DeviceHandle, GlobalContext};

use crate::error::CliError;
use crate::transport::{ControlTransport, TransportError};

/// Timeout of one control transfer.
pub const TRANSFER_TIMEOUT: Duration = Duration::from_millis(1000);

/// A connected, supported device.
pub struct DetectedDevice {
    /// USB bus number.
    pub bus: u8,
    /// USB device address.
    pub address: u8,
    /// Product definition row matched by VID/PID.
    pub product: ProductDefinition,
    device: Device<GlobalContext>,
}

/// Auto-detect priority: mk2 first.
pub const fn autodetect_rank(variant: Variant) -> u8 {
    match variant {
        Variant::Mk2 => 0,
        Variant::Mk1 => 1,
    }
}

/// Enumerate supported devices, mk2 first, then by bus and address.
pub fn enumerate() -> Result<Vec<DetectedDevice>, CliError> {
    let list = rusb::devices().map_err(|source| CliError::Usb {
        context: "cannot enumerate USB devices",
        source,
    })?;
    let mut found = Vec::new();
    for device in list.iter() {
        let Ok(desc) = device.device_descriptor() else {
            continue;
        };
        if desc.vendor_id() != VID {
            continue;
        }
        if let Ok(product) = ProductDefinition::lookup(desc.vendor_id(), desc.product_id()) {
            found.push(DetectedDevice {
                bus: device.bus_number(),
                address: device.address(),
                product,
                device,
            });
        }
    }
    found.sort_by_key(|d| (autodetect_rank(d.product.variant), d.bus, d.address));
    Ok(found)
}

/// The auto-detected device (mk2 preferred).
pub fn select_primary() -> Result<DetectedDevice, CliError> {
    enumerate()?.into_iter().next().ok_or(CliError::NoDevice)
}

/// One configuration descriptor read by index, reduced to what the control
/// path uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigCandidate {
    /// `bConfigurationValue`.
    pub configuration_value: u8,
    /// Every interface alternate setting, with its class-specific bytes.
    pub interfaces: Vec<InterfaceInfo>,
}

/// Choose the configuration to use when the active one cannot be looked up.
///
/// libusb on macOS answers `NotFound` for the active configuration while the
/// process has not opened the device (Linux reads it from sysfs), and a
/// standard GET_CONFIGURATION is not usable there. `candidates` are the
/// configuration descriptors that could be read by index. If there is at
/// least one and they all agree (same `bConfigurationValue` and identical
/// interface list), the active configuration cannot be anything else, so it
/// is returned. If none were read or any two differ, nothing is guessed.
pub fn select_unambiguous_config(candidates: Vec<ConfigCandidate>) -> Option<ConfigCandidate> {
    let mut iter = candidates.into_iter();
    let first = iter.next()?;
    iter.all(|c| c == first).then_some(first)
}

/// Whether an error from the active-configuration lookup allows reading the
/// configuration descriptors by index. Only `NotFound` does; every other
/// error is reported as is.
pub fn allows_indexed_fallback(active_error: rusb::Error) -> bool {
    active_error == rusb::Error::NotFound
}

fn interface_list(config: &ConfigDescriptor) -> Vec<InterfaceInfo> {
    let mut interfaces = Vec::new();
    for interface in config.interfaces() {
        for alt in interface.descriptors() {
            interfaces.push(InterfaceInfo {
                number: alt.interface_number(),
                alt_setting: alt.setting_number(),
                class: alt.class_code(),
                subclass: alt.sub_class_code(),
                protocol: alt.protocol_code(),
                extra: alt.extra().to_vec(),
            });
        }
    }
    interfaces
}

impl DetectedDevice {
    /// Interfaces of the active configuration, from libusb's cached
    /// descriptors (no control request is sent, nothing is claimed). If
    /// libusb reports the active configuration as `NotFound`, every
    /// configuration descriptor is read by index and used only when they all
    /// agree ([`select_unambiguous_config`]); otherwise the original error is
    /// returned.
    pub fn interfaces(&self) -> Result<Vec<InterfaceInfo>, CliError> {
        let active_error = match self.device.active_config_descriptor() {
            Ok(config) => return Ok(interface_list(&config)),
            Err(error) => error,
        };
        let original = || CliError::Usb {
            context: "cannot read the active configuration descriptor",
            source: active_error,
        };
        if !allows_indexed_fallback(active_error) {
            return Err(original());
        }
        let count = self
            .device
            .device_descriptor()
            .map_err(|source| CliError::Usb {
                context: "cannot read the device descriptor",
                source,
            })?
            .num_configurations();
        let candidates = (0..count)
            .filter_map(|index| self.device.config_descriptor(index).ok())
            .map(|config| ConfigCandidate {
                configuration_value: config.number(),
                interfaces: interface_list(&config),
            })
            .collect();
        select_unambiguous_config(candidates)
            .map(|chosen| chosen.interfaces)
            .ok_or_else(original)
    }

    /// Open the device and claim the control interface found in the
    /// descriptor.
    pub fn open_control(&self, interfaces: &[InterfaceInfo]) -> Result<RusbTransport, CliError> {
        let control = find_control_interface(interfaces)?;
        RusbTransport::claim(&self.device, control)
    }
}

/// Live transport holding a claim on the DFU interface only. The claim is
/// released when the value is dropped (success, error or early return).
pub struct RusbTransport {
    handle: DeviceHandle<GlobalContext>,
    interface: u8,
}

impl RusbTransport {
    /// Open `device` and claim `control` (the DFU interface). No kernel
    /// driver is detached; if the interface is busy the claim fails.
    pub fn claim(
        device: &Device<GlobalContext>,
        control: ControlInterface,
    ) -> Result<Self, CliError> {
        if control.number <= 2 {
            return Err(ControlError::ControlInterfaceReserved {
                number: control.number,
            }
            .into());
        }
        let handle = device.open().map_err(|source| CliError::Usb {
            context: "cannot open the device",
            source,
        })?;
        handle
            .claim_interface(control.number)
            .map_err(|source| CliError::Usb {
                context: "cannot claim the DFU control interface (no kernel driver is detached)",
                source,
            })?;
        Ok(RusbTransport {
            handle,
            interface: control.number,
        })
    }
}

impl Drop for RusbTransport {
    fn drop(&mut self) {
        // Best effort: the handle is closed right after, which also releases.
        let _ = self.handle.release_interface(self.interface);
    }
}

fn transport_error(setup: &SetupPacket, source: rusb::Error) -> TransportError {
    TransportError {
        setup: *setup,
        cause: Box::new(source),
    }
}

impl ControlTransport for RusbTransport {
    fn control_in(&mut self, setup: &SetupPacket) -> Result<Vec<u8>, TransportError> {
        let mut buf = vec![0u8; usize::from(setup.w_length)];
        let n = self
            .handle
            .read_control(
                setup.bm_request_type(),
                setup.b_request(),
                setup.w_value(),
                setup.w_index(),
                &mut buf,
                TRANSFER_TIMEOUT,
            )
            .map_err(|e| transport_error(setup, e))?;
        buf.truncate(n);
        Ok(buf)
    }

    fn control_out(&mut self, setup: &SetupPacket, payload: &[u8]) -> Result<(), TransportError> {
        self.handle
            .write_control(
                setup.bm_request_type(),
                setup.b_request(),
                setup.w_value(),
                setup.w_index(),
                payload,
                TRANSFER_TIMEOUT,
            )
            .map_err(|e| transport_error(setup, e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use id14_protocol::fixtures::mk2_synthetic_interfaces;

    fn candidate(configuration_value: u8) -> ConfigCandidate {
        ConfigCandidate {
            configuration_value,
            interfaces: mk2_synthetic_interfaces(),
        }
    }

    #[test]
    fn identical_candidates_are_selected() {
        // mk2 measured shape: bNumConfigurations 2, both indices read back
        // bConfigurationValue 1 with identical content
        let chosen = select_unambiguous_config(vec![candidate(1), candidate(1)]);
        assert_eq!(chosen, Some(candidate(1)));
        assert_eq!(
            select_unambiguous_config(vec![candidate(1)]),
            Some(candidate(1))
        );
    }

    #[test]
    fn no_readable_candidate_is_not_guessed_around() {
        assert_eq!(select_unambiguous_config(Vec::new()), None);
    }

    #[test]
    fn differing_configuration_values_are_refused() {
        assert_eq!(
            select_unambiguous_config(vec![candidate(1), candidate(2)]),
            None
        );
    }

    #[test]
    fn differing_interface_content_is_refused() {
        let mut other = candidate(1);
        other.interfaces[0].extra.push(0);
        assert_eq!(
            select_unambiguous_config(vec![candidate(1), other.clone()]),
            None
        );
        let mut fewer = candidate(1);
        fewer.interfaces.pop();
        assert_eq!(
            select_unambiguous_config(vec![candidate(1), candidate(1), fewer]),
            None
        );
    }

    #[test]
    fn only_not_found_allows_the_indexed_fallback() {
        assert!(allows_indexed_fallback(rusb::Error::NotFound));
        for error in [
            rusb::Error::Access,
            rusb::Error::NoDevice,
            rusb::Error::Io,
            rusb::Error::NotSupported,
            rusb::Error::Busy,
        ] {
            assert!(!allows_indexed_fallback(error));
        }
    }

    #[test]
    fn autodetect_prefers_mk2() {
        assert!(autodetect_rank(Variant::Mk2) < autodetect_rank(Variant::Mk1));
    }
}
