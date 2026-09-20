//! Host-independent assertions of the pinned protocol constants, exercised
//! through the public API only. No device, no USB, no host dependency.

use id14_protocol::{
    ControlRequest, EvidenceStatus, ExtensionCode, ProductDefinition, ProtocolError, RequestKind,
    Variant, PID_MK1, PID_MK2, PRODUCTS, PROTOCOL_EVIDENCE_STATUS, VID,
};

#[test]
fn product_table_pins() {
    assert_eq!(VID, 0x2708);
    assert_eq!(PID_MK1, 0x0002);
    assert_eq!(PID_MK2, 0x0008);
    assert!(PRODUCTS
        .iter()
        .any(|p| p.variant == Variant::Mk1 && p.vid == 0x2708 && p.pid == 0x0002));
    assert!(PRODUCTS
        .iter()
        .any(|p| p.variant == Variant::Mk2 && p.vid == 0x2708 && p.pid == 0x0008));
    assert!(PRODUCTS.iter().all(|p| p.vid == 0x2708));
    assert_eq!(
        ProductDefinition::try_from((0x2708u16, 0x0008u16)).map(|p| p.variant),
        Ok(Variant::Mk2)
    );
    assert!(matches!(
        ProductDefinition::lookup(0x2708, 0x0003),
        Err(ProtocolError::UnknownProduct { .. })
    ));
}

#[test]
fn request_header_pins() {
    assert_eq!(RequestKind::Get.header_u16(), 0x01a1);
    assert_eq!(RequestKind::Set.header_u16(), 0x0121);
    assert_eq!(RequestKind::GetMem.header_u16(), 0x03a1);
    assert_eq!(ControlRequest::get([]).to_bytes(), vec![0xa1, 0x01]);
    assert_eq!(ControlRequest::set([]).to_bytes(), vec![0x21, 0x01]);
    assert_eq!(ControlRequest::get_mem([]).to_bytes(), vec![0xa1, 0x03]);
}

#[test]
fn extension_code_pins() {
    assert_eq!(ExtensionCode::RoutingMatrix.code(), 0);
    assert_eq!(ExtensionCode::MonitorController.code(), 1);
    assert_eq!(ExtensionCode::MonoMixController.code(), 2);
    assert_eq!(
        ExtensionCode::try_from(2u8),
        Ok(ExtensionCode::MonoMixController)
    );
    assert_eq!(
        ExtensionCode::try_from(7u8),
        Err(ProtocolError::UnknownExtensionCode(7))
    );
}

#[test]
fn evidence_status_pins() {
    assert_eq!(
        PROTOCOL_EVIDENCE_STATUS,
        EvidenceStatus::StaticAnalysisInferredUnverified
    );
    assert!(!PROTOCOL_EVIDENCE_STATUS.is_hardware_confirmed());
    assert_eq!(
        PROTOCOL_EVIDENCE_STATUS.to_string(),
        "static_analysis_inferred_unverified"
    );
}
