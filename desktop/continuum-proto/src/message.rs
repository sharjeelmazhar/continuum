//! Message envelope and typed bodies (see `docs/PROTOCOL.md`).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ProtoError;

pub const HELLO: &str = "hello";
pub const PING: &str = "ping";
pub const PONG: &str = "pong";
pub const UNPAIR: &str = "unpair";
pub const ERROR: &str = "error";
pub const PAIR_REQUEST: &str = "pair.request";
pub const PAIR_ACCEPT: &str = "pair.accept";
pub const PAIR_REJECT: &str = "pair.reject";
pub const BATTERY_STATUS: &str = "battery.status";

/// Capabilities this build understands.
pub const CAPS: &[&str] = &["battery"];

/// One control message: `{"type": "...", "body": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "empty_body")]
    pub body: Value,
}

fn empty_body() -> Value {
    Value::Object(Default::default())
}

impl Message {
    /// A message with an empty body.
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            body: empty_body(),
        }
    }

    pub fn with_body<T: Serialize>(kind: &str, body: &T) -> Result<Self, ProtoError> {
        Ok(Self {
            kind: kind.to_owned(),
            body: serde_json::to_value(body)?,
        })
    }

    /// Parses the body as `T`. Unknown fields are ignored; missing required fields are an error.
    pub fn body<T: DeserializeOwned>(&self) -> Result<T, ProtoError> {
        Ok(T::deserialize(&self.body)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Phone,
    Tablet,
    Laptop,
    Desktop,
}

impl DeviceType {
    /// Symbolic icon name for GTK.
    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Phone => "phone-symbolic",
            Self::Tablet => "tablet-symbolic",
            Self::Laptop => "computer-symbolic",
            Self::Desktop => "computer-symbolic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub v: u32,
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: DeviceType,
    #[serde(default)]
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: DeviceType,
    pub proof: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairAccept {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: DeviceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairReject {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub level: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    #[serde(default)]
    pub msg: String,
}

/// Peer-supplied display names: trimmed, control characters removed, at most 64 characters.
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Unnamed device".to_owned()
    } else {
        cleaned.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_body_is_empty_object() {
        let m: Message = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert_eq!(m, Message::new(PING));
    }

    #[test]
    fn typed_body_ignores_unknown_fields() {
        let m: Message = serde_json::from_str(
            r#"{"type":"battery.status","body":{"level":82,"charging":true,"future":1}}"#,
        )
        .unwrap();
        assert_eq!(
            m.body::<BatteryStatus>().unwrap(),
            BatteryStatus {
                level: 82,
                charging: true
            }
        );
    }

    #[test]
    fn wrong_body_is_an_error() {
        let m: Message =
            serde_json::from_str(r#"{"type":"battery.status","body":{"level":"x"}}"#).unwrap();
        assert!(m.body::<BatteryStatus>().is_err());
    }

    #[test]
    fn device_type_is_lowercase() {
        let h = Hello {
            v: 1,
            id: "a".into(),
            name: "n".into(),
            device_type: DeviceType::Phone,
            caps: vec![],
        };
        assert!(
            serde_json::to_string(&h)
                .unwrap()
                .contains(r#""type":"phone""#)
        );
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Pixel\u{0} 8\n "), "Pixel 8");
        assert_eq!(clean_name("\u{7}"), "Unnamed device");
        assert_eq!(clean_name(&"x".repeat(100)).len(), 64);
    }
}
