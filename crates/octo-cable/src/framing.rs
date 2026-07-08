//! caBLE tunnel framing.
//!
//! Per Chromium's `cable/fido_tunnel_device.cc`, all post-handshake
//! application messages are wrapped in a [`CableFrame`] with:
//!
//! | Offset | Size | Field             |
//! |--------|------|-------------------|
//! | 0      | 1    | `protocol_version` (always 1) |
//! | 1      | 1    | `message_type` (0=KeepAlive, 1=Ctap, 2=Shutdown) |
//! | 2      | 2    | `data_length` (big-endian u16) |
//! | 4      | N    | `data` |
//!
//! The whole frame is AES-256-GCM encrypted by the [`Crypter`](crate::noise::Crypter)
//! before being sent as a WebSocket binary frame.
//!
//! ## Reference
//!
//! <https://source.chromium.org/chromium/chromium/src/+/main:device/fido/cable/fido_tunnel_device.cc;l=270-296>

/// Application message type codes used in [`CableFrame::message_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Reserved / no-op ping.
    KeepAlive = 0,
    /// CTAP2 CBOR command or response.
    Ctap = 1,
    /// Tunnel shutdown. Receiver closes the WebSocket.
    Shutdown = 2,
}

impl MessageType {
    fn from_u8(b: u8) -> Result<Self, crate::error::CableError> {
        match b {
            0 => Ok(MessageType::KeepAlive),
            1 => Ok(MessageType::Ctap),
            2 => Ok(MessageType::Shutdown),
            other => Err(crate::error::CableError::Cbor(format!(
                "unknown message_type byte 0x{other:02x}"
            ))),
        }
    }
}

/// A single caBLE application-layer frame. Wraps CTAP2 commands and
/// responses between the noise-encrypted tunnel endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CableFrame {
    /// Protocol version. caBLE v2.x is `1`.
    pub protocol_version: u8,
    /// What kind of payload is in `data`.
    pub message_type: MessageType,
    /// Payload bytes (raw CTAP2 CBOR for `MessageType::Ctap`).
    pub data: Vec<u8>,
}

impl CableFrame {
    /// Encode to the on-the-wire byte layout (4-byte header + data).
    /// Caller is responsible for AES-GCM encryption afterwards.
    pub fn to_bytes(&self) -> Vec<u8> {
        let len = self.data.len();
        assert!(len <= u16::MAX as usize, "data > u16::MAX");
        let mut out = Vec::with_capacity(4 + len);
        out.push(self.protocol_version);
        out.push(self.message_type as u8);
        out.extend_from_slice(&(len as u16).to_be_bytes());
        out.extend_from_slice(&self.data);
        out
    }

    /// Decode from a 4-byte-header byte slice. The length field is
    /// validated against `bytes.len()`. Returns an error if the buffer
    /// is malformed.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, crate::error::CableError> {
        if bytes.len() < 4 {
            return Err(crate::error::CableError::Cbor(format!(
                "frame too short: {} bytes",
                bytes.len()
            )));
        }
        let protocol_version = bytes[0];
        let message_type = MessageType::from_u8(bytes[1])?;
        let len = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
        if bytes.len() != 4 + len {
            return Err(crate::error::CableError::Cbor(format!(
                "frame length mismatch: header says {}, have {}",
                len,
                bytes.len() - 4
            )));
        }
        Ok(CableFrame {
            protocol_version,
            message_type,
            data: bytes[4..].to_vec(),
        })
    }
}

/// SHUTDOWN frame: protocol_version=1, message_type=2, empty data.
/// Sent by either side to politely terminate the tunnel.
pub const SHUTDOWN_COMMAND_BYTES: [u8; 4] = [0x01, 0x02, 0x00, 0x00];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ctap_frame() {
        let f = CableFrame {
            protocol_version: 1,
            message_type: MessageType::Ctap,
            data: vec![0x01, 0x02, 0x03, 0x04, 0x05],
        };
        let bytes = f.to_bytes();
        assert_eq!(
            bytes,
            vec![0x01, 0x01, 0x00, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05]
        );
        let back = CableFrame::from_bytes(&bytes).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn round_trip_shutdown_command() {
        let f = CableFrame {
            protocol_version: 1,
            message_type: MessageType::Shutdown,
            data: vec![],
        };
        let bytes = f.to_bytes();
        assert_eq!(bytes, SHUTDOWN_COMMAND_BYTES.to_vec());
        let back = CableFrame::from_bytes(&bytes).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn from_bytes_rejects_short_buffer() {
        let err = CableFrame::from_bytes(&[0x01, 0x01]).unwrap_err();
        assert!(matches!(err, crate::error::CableError::Cbor(_)));
    }

    #[test]
    fn from_bytes_rejects_length_mismatch() {
        // Header claims 5 bytes but buffer has 3
        let err = CableFrame::from_bytes(&[0x01, 0x01, 0x00, 0x05, 0x01, 0x02, 0x03]).unwrap_err();
        assert!(matches!(err, crate::error::CableError::Cbor(_)));
    }

    #[test]
    fn from_bytes_rejects_unknown_message_type() {
        let err = CableFrame::from_bytes(&[0x01, 0x99, 0x00, 0x00]).unwrap_err();
        assert!(matches!(err, crate::error::CableError::Cbor(_)));
    }
}
