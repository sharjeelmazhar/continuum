//! Length-prefixed framing: `u32` big-endian length + JSON body, at most 1 MiB (ADR-0006).

use crate::ProtoError;
use crate::message::Message;

/// Largest accepted frame body, in bytes.
pub const MAX_FRAME: usize = 1 << 20;

/// Serializes a message into one frame.
pub fn encode(msg: &Message) -> Result<Vec<u8>, ProtoError> {
    let body = serde_json::to_vec(msg)?;
    if body.len() > MAX_FRAME {
        return Err(ProtoError::FrameTooLarge(body.len()));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Incremental decoder: feed bytes as they arrive, take complete messages out.
#[derive(Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends received bytes.
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Returns the next complete message, `None` if more bytes are needed, or an error. The
    /// length is checked before the body is buffered, so a hostile peer can't make us allocate
    /// more than the limit. After an error the connection must be closed.
    pub fn next_message(&mut self) -> Result<Option<Message>, ProtoError> {
        let Some(len) = self.pending_len()? else {
            return Ok(None);
        };
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let msg = serde_json::from_slice(&self.buf[4..4 + len])?;
        self.buf.drain(..4 + len);
        Ok(Some(msg))
    }

    /// Hands over bytes received after the last complete frame (for switching to raw data).
    pub fn take_remaining(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }

    /// Length of the frame being received, once its header is complete.
    pub fn pending_len(&self) -> Result<Option<usize>, ProtoError> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if len > MAX_FRAME {
            return Err(ProtoError::FrameTooLarge(len));
        }
        Ok(Some(len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn msg(kind: &str) -> Message {
        Message::with_body(kind, &json!({"level": 5})).unwrap()
    }

    #[test]
    fn round_trip() {
        let mut d = FrameDecoder::new();
        d.push(&encode(&msg("a.b")).unwrap());
        d.push(&encode(&msg("c.d")).unwrap());
        assert_eq!(d.next_message().unwrap().unwrap().kind, "a.b");
        assert_eq!(d.next_message().unwrap().unwrap().kind, "c.d");
        assert!(d.next_message().unwrap().is_none());
    }

    #[test]
    fn matches_shared_vector() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../tests/vectors.json")).unwrap();
        let m: Message = serde_json::from_str(v["frame"]["json"].as_str().unwrap()).unwrap();
        assert_eq!(
            hex::encode(encode(&m).unwrap()),
            v["frame"]["hex"].as_str().unwrap()
        );
    }

    #[test]
    fn rejects_oversized_header_before_body() {
        let mut d = FrameDecoder::new();
        d.push(&((MAX_FRAME as u32) + 1).to_be_bytes());
        assert!(matches!(
            d.next_message(),
            Err(ProtoError::FrameTooLarge(_))
        ));
    }

    #[test]
    fn rejects_bad_json() {
        let mut d = FrameDecoder::new();
        d.push(&3u32.to_be_bytes());
        d.push(b"{x}");
        assert!(matches!(d.next_message(), Err(ProtoError::BadMessage(_))));
    }

    proptest! {
        #[test]
        fn any_split_decodes_identically(split in 0usize..60) {
            let bytes = encode(&msg("battery.status")).unwrap();
            let split = split.min(bytes.len());
            let mut d = FrameDecoder::new();
            d.push(&bytes[..split]);
            let first = d.next_message().unwrap();
            d.push(&bytes[split..]);
            let got = first.or_else(|| d.next_message().unwrap()).unwrap();
            prop_assert_eq!(got, msg("battery.status"));
        }

        #[test]
        fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..512)) {
            let mut d = FrameDecoder::new();
            d.push(&data);
            while let Ok(Some(_)) = d.next_message() {}
        }
    }
}
