//! Clipboard sync (PROTOCOL.md, "Clipboard"): a copied text or image travels as `clipboard`
//! messages of at most [`PART_BYTES`] each, so every frame stays under the 1 MiB limit.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::digest::{SHA256, digest};

use crate::message::ClipboardPart;

/// Largest clipboard content sent or accepted.
pub const MAX_BYTES: usize = 12 << 20;
/// Raw bytes per part (base64 makes it about 683 KiB on the wire).
pub const PART_BYTES: usize = 512 << 10;
pub const TEXT: &str = "text/plain";
pub const IMAGE_TYPES: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    pub mime: String,
    pub data: Vec<u8>,
}

impl Content {
    pub fn text(s: &str) -> Self {
        Self {
            mime: TEXT.into(),
            data: s.as_bytes().to_vec(),
        }
    }

    /// Identifies the content, so a device never sends back what it just received.
    pub fn digest(&self) -> [u8; 32] {
        let mut v = self.mime.as_bytes().to_vec();
        v.push(0);
        v.extend_from_slice(&self.data);
        let mut out = [0u8; 32];
        out.copy_from_slice(digest(&SHA256, &v).as_ref());
        out
    }

    fn valid(&self) -> bool {
        self.data.len() <= MAX_BYTES
            && match self.mime.as_str() {
                TEXT => std::str::from_utf8(&self.data).is_ok(),
                m => IMAGE_TYPES.contains(&m),
            }
    }
}

/// Splits content into messages, or `None` if it's too large or of an unsupported type.
pub fn split(c: &Content) -> Option<Vec<ClipboardPart>> {
    if !c.valid() || c.data.is_empty() {
        return None;
    }
    let id = hex::encode(&c.digest()[..8]);
    let chunks: Vec<&[u8]> = c.data.chunks(PART_BYTES).collect();
    let parts = chunks.len() as u32;
    Some(
        chunks
            .iter()
            .enumerate()
            .map(|(i, chunk)| ClipboardPart {
                id: id.clone(),
                mime: c.mime.clone(),
                size: c.data.len() as u64,
                part: i as u32,
                parts,
                data: STANDARD.encode(chunk),
            })
            .collect(),
    )
}

/// Collects parts (in order, as sent on one connection) back into content.
/// Anything inconsistent drops the content being collected.
#[derive(Default)]
pub struct Assembler {
    current: Option<(ClipboardPart, Vec<u8>)>,
}

impl Assembler {
    pub fn push(&mut self, p: ClipboardPart) -> Option<Content> {
        let size = usize::try_from(p.size).ok()?;
        let expected_parts = size.div_ceil(PART_BYTES) as u32;
        let ok_header = size > 0 && size <= MAX_BYTES && p.parts == expected_parts;
        let Ok(bytes) = STANDARD.decode(&p.data) else {
            self.current = None;
            return None;
        };
        let want = if p.part + 1 == p.parts {
            size - PART_BYTES * (p.parts as usize - 1)
        } else {
            PART_BYTES
        };
        if !ok_header || p.part >= p.parts || bytes.len() != want {
            self.current = None;
            return None;
        }
        let mut buf = if p.part == 0 {
            Vec::with_capacity(size)
        } else {
            match self.current.take() {
                Some((first, buf))
                    if first.id == p.id
                        && first.mime == p.mime
                        && first.size == p.size
                        && buf.len() == PART_BYTES * p.part as usize =>
                {
                    buf
                }
                _ => return None,
            }
        };
        buf.extend_from_slice(&bytes);
        if p.part + 1 < p.parts {
            self.current = Some((p, buf));
            return None;
        }
        let c = Content {
            mime: p.mime,
            data: buf,
        };
        c.valid().then_some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(c: &Content) -> Option<Content> {
        let mut a = Assembler::default();
        let mut out = None;
        for p in split(c)? {
            out = a.push(p);
        }
        out
    }

    #[test]
    fn text_and_large_image_roundtrip() {
        let t = Content::text("héllo 👋");
        assert_eq!(roundtrip(&t), Some(t.clone()));
        assert_eq!(split(&t).unwrap().len(), 1);
        let img = Content {
            mime: "image/png".into(),
            data: (0..PART_BYTES * 2 + 7).map(|i| i as u8).collect(),
        };
        assert_eq!(split(&img).unwrap().len(), 3);
        assert_eq!(roundtrip(&img), Some(img));
    }

    #[test]
    fn rejects_bad_content() {
        assert!(split(&Content::text("")).is_none());
        let exe = Content {
            mime: "application/x-sh".into(),
            data: b"rm -rf".to_vec(),
        };
        assert!(split(&exe).is_none());
        let huge = Content {
            mime: "image/png".into(),
            data: vec![0; MAX_BYTES + 1],
        };
        assert!(split(&huge).is_none());
        let bad_utf8 = Content {
            mime: TEXT.into(),
            data: vec![0xff, 0xfe],
        };
        assert!(split(&bad_utf8).is_none());
    }

    #[test]
    fn hostile_parts_are_dropped() {
        let img = Content {
            mime: "image/png".into(),
            data: vec![7; PART_BYTES + 10],
        };
        let parts = split(&img).unwrap();
        let mut a = Assembler::default();
        // Out of order: second part first.
        assert!(a.push(parts[1].clone()).is_none());
        // Lying about the size.
        let mut liar = parts[0].clone();
        liar.size = 3 * PART_BYTES as u64;
        assert!(a.push(liar).is_none());
        // Claims to be bigger than allowed.
        let mut big = parts[0].clone();
        big.size = MAX_BYTES as u64 + 1;
        big.parts = big.size.div_ceil(PART_BYTES as u64) as u32;
        assert!(a.push(big).is_none());
        // Mixed ids.
        assert!(a.push(parts[0].clone()).is_none());
        let mut other = parts[1].clone();
        other.id = "0000000000000000".into();
        assert!(a.push(other).is_none());
        // A clean sequence still works afterwards.
        assert!(a.push(parts[0].clone()).is_none());
        assert_eq!(a.push(parts[1].clone()), Some(img));
    }
}
