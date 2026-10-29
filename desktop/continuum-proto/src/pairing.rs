//! Pairing: the QR link, the one-time secret and the HMAC proof (see `docs/design/06-security.md`).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};

use crate::ProtoError;
use crate::identity::{Fingerprint, fingerprint_from_hex};

pub type Secret = [u8; 16];

pub fn random_secret() -> Secret {
    let mut s = [0u8; 16];
    SystemRandom::new().fill(&mut s).expect("system RNG failed");
    s
}

/// HMAC-SHA256(secret, fp_desktop ‖ fp_phone).
pub fn proof(secret: &Secret, fp_desktop: &Fingerprint, fp_phone: &Fingerprint) -> [u8; 32] {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let mut ctx = hmac::Context::with_key(&key);
    ctx.update(fp_desktop);
    ctx.update(fp_phone);
    ctx.sign()
        .as_ref()
        .try_into()
        .expect("HMAC-SHA256 is 32 bytes")
}

/// Checks a hex proof in constant time.
pub fn verify_proof(
    secret: &Secret,
    fp_desktop: &Fingerprint,
    fp_phone: &Fingerprint,
    proof_hex: &str,
) -> bool {
    let Ok(tag) = hex::decode(proof_hex) else {
        return false;
    };
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let mut msg = Vec::with_capacity(64);
    msg.extend_from_slice(fp_desktop);
    msg.extend_from_slice(fp_phone);
    hmac::verify(&key, &msg, &tag).is_ok()
}

/// Contents of the pairing QR code.
#[derive(Debug, Clone, PartialEq)]
pub struct PairUri {
    pub hosts: Vec<String>,
    pub port: u16,
    pub fp: Fingerprint,
    pub secret: Secret,
    pub name: String,
}

impl PairUri {
    pub fn to_uri(&self) -> String {
        format!(
            "continuum://pair?v=1&h={}&p={}&fp={}&s={}&n={}",
            self.hosts.join(","),
            self.port,
            hex::encode(self.fp),
            URL_SAFE_NO_PAD.encode(self.secret),
            percent_encode(&self.name)
        )
    }

    pub fn parse(uri: &str) -> Result<Self, ProtoError> {
        let query = uri
            .strip_prefix("continuum://pair?")
            .ok_or(ProtoError::BadUri("not a pairing link"))?;
        let (mut v, mut h, mut p, mut fp, mut s, mut n) = (None, None, None, None, None, None);
        for pair in query.split('&') {
            let (k, val) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "v" => v = Some(val),
                "h" => h = Some(val),
                "p" => p = Some(val),
                "fp" => fp = Some(val),
                "s" => s = Some(val),
                "n" => n = Some(val),
                _ => {}
            }
        }
        if v != Some("1") {
            return Err(ProtoError::BadUri("unsupported version"));
        }
        let hosts: Vec<String> = h
            .ok_or(ProtoError::BadUri("missing h"))?
            .split(',')
            .filter(|x| !x.is_empty())
            .map(str::to_owned)
            .collect();
        if hosts.is_empty() || hosts.len() > 8 {
            return Err(ProtoError::BadUri("bad host list"));
        }
        let port = p
            .and_then(|p| p.parse().ok())
            .filter(|&p| p != 0)
            .ok_or(ProtoError::BadUri("bad port"))?;
        let fp = fp
            .and_then(fingerprint_from_hex)
            .ok_or(ProtoError::BadUri("bad fingerprint"))?;
        let secret = s
            .and_then(|s| URL_SAFE_NO_PAD.decode(s).ok())
            .and_then(|b| Secret::try_from(b).ok())
            .ok_or(ProtoError::BadUri("bad secret"))?;
        let name = percent_decode(n.unwrap_or("")).ok_or(ProtoError::BadUri("bad name"))?;
        Ok(Self {
            hosts,
            port,
            fp,
            secret,
            name,
        })
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hexpair = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hexpair, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../tests/vectors.json")).unwrap()
    }

    #[test]
    fn proof_matches_shared_vector() {
        let v = &vectors()["pairing"];
        let secret: Secret = URL_SAFE_NO_PAD
            .decode(v["secret_b64url"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let fd = fingerprint_from_hex(v["fp_desktop"].as_str().unwrap()).unwrap();
        let fp = fingerprint_from_hex(v["fp_phone"].as_str().unwrap()).unwrap();
        let expected = v["proof"].as_str().unwrap();
        assert_eq!(hex::encode(proof(&secret, &fd, &fp)), expected);
        assert!(verify_proof(&secret, &fd, &fp, expected));
        assert!(!verify_proof(&secret, &fp, &fd, expected), "order matters");
        assert!(!verify_proof(&random_secret(), &fd, &fp, expected));
        assert!(!verify_proof(&secret, &fd, &fp, "zz"));
    }

    #[test]
    fn uri_matches_shared_vector_both_ways() {
        let v = &vectors()["uri"];
        let text = v["text"].as_str().unwrap();
        let u = PairUri::parse(text).unwrap();
        assert_eq!(u.hosts, ["192.0.2.10", "198.51.100.7"]);
        assert_eq!(u.port, 47820);
        assert_eq!(u.name, "My Desktop & Co");
        assert_eq!(u.to_uri(), text);
    }

    #[test]
    fn bad_uris_are_rejected() {
        let good = vectors()["uri"]["text"].as_str().unwrap().to_owned();
        for bad in [
            "https://example.com".to_owned(),
            good.replace("v=1", "v=2"),
            good.replace("p=47820", "p=0"),
            good.replace("p=47820", "p=99999"),
            good.replace("&s=AAECAwQFBgcICQoLDA0ODw", "&s=AAEC"),
            good.replace("fp=bac8", "fp=zz"),
            good.replace("n=My", "n=%G1"),
        ] {
            assert!(PairUri::parse(&bad).is_err(), "{bad}");
        }
    }
}
