//! Control-request framing.
//!
//! Only the leading 16-bit header of each request kind is pinned:
//! GET `0x01a1`, SET `0x0121`, GET_MEM `0x03a1`, serialised little-endian
//! (`a1 01` / `21 01` / `a1 03`). Everything after the header (selector,
//! channel, payload layout) is not yet reconstructed, so [`ControlRequest`]
//! carries it as an opaque byte slice supplied by the caller.

/// Kind of control request, identified by its pinned leading header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestKind {
    /// `sendGetRequest` — header `0x01a1`.
    Get,
    /// `sendSetRequest` — header `0x0121`.
    Set,
    /// `sendGetMemRequest` — header `0x03a1`.
    GetMem,
}

impl RequestKind {
    /// The leading 16-bit header of this request kind.
    pub const fn header_u16(self) -> u16 {
        match self {
            RequestKind::Get => 0x01a1,
            RequestKind::Set => 0x0121,
            RequestKind::GetMem => 0x03a1,
        }
    }

    /// The header as it appears on the wire (little-endian).
    pub const fn header_bytes(self) -> [u8; 2] {
        self.header_u16().to_le_bytes()
    }
}

/// A control request: pinned header followed by caller-supplied bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControlRequest {
    kind: RequestKind,
    body: Vec<u8>,
}

impl ControlRequest {
    /// Build a request of `kind` whose bytes after the header are `body`.
    pub fn new(kind: RequestKind, body: impl Into<Vec<u8>>) -> Self {
        ControlRequest {
            kind,
            body: body.into(),
        }
    }

    /// `sendGetRequest` framing.
    pub fn get(body: impl Into<Vec<u8>>) -> Self {
        Self::new(RequestKind::Get, body)
    }

    /// `sendSetRequest` framing.
    pub fn set(body: impl Into<Vec<u8>>) -> Self {
        Self::new(RequestKind::Set, body)
    }

    /// `sendGetMemRequest` framing.
    pub fn get_mem(body: impl Into<Vec<u8>>) -> Self {
        Self::new(RequestKind::GetMem, body)
    }

    /// The request kind.
    pub const fn kind(&self) -> RequestKind {
        self.kind
    }

    /// The bytes following the header.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The full on-wire byte sequence: header (LE) followed by the body.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + self.body.len());
        out.extend_from_slice(&self.kind.header_bytes());
        out.extend_from_slice(&self.body);
        out
    }
}

impl From<ControlRequest> for Vec<u8> {
    fn from(r: ControlRequest) -> Self {
        r.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_headers() {
        assert_eq!(RequestKind::Get.header_u16(), 0x01a1);
        assert_eq!(RequestKind::Set.header_u16(), 0x0121);
        assert_eq!(RequestKind::GetMem.header_u16(), 0x03a1);
    }

    #[test]
    fn header_wire_bytes_le() {
        assert_eq!(RequestKind::Get.header_bytes(), [0xa1, 0x01]);
        assert_eq!(RequestKind::Set.header_bytes(), [0x21, 0x01]);
        assert_eq!(RequestKind::GetMem.header_bytes(), [0xa1, 0x03]);
    }

    #[test]
    fn request_bytes_are_header_then_body() {
        assert_eq!(ControlRequest::get([]).to_bytes(), [0xa1, 0x01]);
        assert_eq!(
            ControlRequest::set([0x10, 0x20]).to_bytes(),
            [0x21, 0x01, 0x10, 0x20]
        );
        let r = ControlRequest::get_mem(vec![0xff]);
        assert_eq!(r.kind(), RequestKind::GetMem);
        assert_eq!(r.body(), &[0xff]);
        assert_eq!(Vec::<u8>::from(r), vec![0xa1, 0x03, 0xff]);
    }
}
