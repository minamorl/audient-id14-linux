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
use rusb::{Device, DeviceHandle, GlobalContext};

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

impl DetectedDevice {
    /// Interfaces of the active configuration, from libusb's cached
    /// descriptor (no control request is sent).
    pub fn interfaces(&self) -> Result<Vec<InterfaceInfo>, CliError> {
        let config = self
            .device
            .active_config_descriptor()
            .map_err(|source| CliError::Usb {
                context: "cannot read the active configuration descriptor",
                source,
            })?;
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
        Ok(interfaces)
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
