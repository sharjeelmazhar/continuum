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
pub const SHARE_OFFER: &str = "share.offer";
pub const SHARE_ACCEPT: &str = "share.accept";
pub const SHARE_REJECT: &str = "share.reject";
pub const SHARE_RESULT: &str = "share.result";
pub const DATA_HELLO: &str = "data.hello";
pub const CLIPBOARD: &str = "clipboard";
pub const PHOTOS_PART: &str = "photos.part";
pub const SMS_CHANGED: &str = "sms.changed";
pub const NOTIF_POSTED: &str = "notif.posted";
pub const NOTIF_REMOVED: &str = "notif.removed";

/// Requests the desktop may send to the phone, with the capability each needs
/// (see "Requests" in `docs/PROTOCOL.md`). Replies are `<type>.reply`.
pub const REQUESTS: &[(&str, &str)] = &[
    ("photos.list", CAP_PHOTOS),
    ("photos.thumbs", CAP_PHOTOS),
    ("photos.fetch", CAP_PHOTOS),
    ("photos.send", CAP_PHOTOS),
    ("files.list", CAP_FILES),
    ("files.fetch", CAP_FILES),
    ("sms.threads", CAP_SMS),
    ("sms.messages", CAP_SMS),
    ("sms.send", CAP_SMS),
];

/// Fire-and-forget messages the desktop may send, with the capability each needs.
pub const COMMANDS: &[(&str, &str)] = &[
    ("notif.dismiss", CAP_NOTIF),
    ("notif.reply", CAP_NOTIF),
    ("notif.action", CAP_NOTIF),
    ("find.ring", CAP_FIND),
    ("find.stop", CAP_FIND),
];

/// Capabilities this build understands.
pub const CAPS: &[&str] = &[
    "battery",
    "share",
    "share.receive",
    "clipboard",
    "photos",
    "sms",
    "notif",
    "files",
    "find",
];
pub const CAP_PHOTOS: &str = "photos";
pub const CAP_SMS: &str = "sms";
pub const CAP_NOTIF: &str = "notif";
pub const CAP_FILES: &str = "files";
pub const CAP_FIND: &str = "find";

/// Capability of a peer that accepts `share.offer` (files sent to it).
pub const CAP_SHARE_RECEIVE: &str = "share.receive";
pub const CAP_CLIPBOARD: &str = "clipboard";

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
    /// Charging power in watts while charging (0 = unknown; phones before 0.5.1 don't send it).
    #[serde(default)]
    pub watts: u16,
    /// A fast charger is plugged in.
    #[serde(default)]
    pub fast: bool,
    /// Minutes until full while charging (-1 = unknown).
    #[serde(default = "unknown")]
    pub full_in: i32,
}

fn unknown() -> i32 {
    -1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    #[serde(default)]
    pub msg: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareOffer {
    pub transfer_id: String,
    pub files: Vec<FileMeta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareAccept {
    pub transfer_id: String,
    pub token: String,
    pub offsets: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareReject {
    pub transfer_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareResult {
    pub transfer_id: String,
    pub ok: bool,
    #[serde(default)]
    pub saved: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataHello {
    pub token: String,
}

/// One part of a copied text or image; see [`crate::clipboard`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPart {
    pub id: String,
    pub mime: String,
    pub size: u64,
    pub part: u32,
    pub parts: u32,
    /// Standard base64 of this part's bytes.
    pub data: String,
}

/// First reply to `photos.fetch`: what follows in `parts` × `photos.part`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoFetchReply {
    pub req: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub parts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoPart {
    pub req: u32,
    pub part: u32,
    /// Standard base64 of this part's bytes (512 KiB raw, the last one may be shorter).
    pub data: String,
}

/// A notification on the phone (`notif.posted`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhoneNotification {
    pub key: String,
    #[serde(default)]
    pub pkg: String,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub time: i64,
    /// Base64 PNG of the app icon, may be empty.
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub reply: bool,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub silent: bool,
}

impl PhoneNotification {
    /// Caps every peer-supplied field; `None` if the key is unusable.
    pub fn sanitized(mut self) -> Option<Self> {
        fn cut(s: &mut String, max: usize) {
            if s.len() > max {
                let mut end = max;
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s.truncate(end);
            }
        }
        if self.key.is_empty() || self.key.len() > 512 {
            return None;
        }
        cut(&mut self.pkg, 256);
        self.app = clean_name(&self.app);
        cut(&mut self.title, 512);
        cut(&mut self.text, 4096);
        if self.icon.len() > 64 * 1024 {
            self.icon.clear();
        }
        self.actions.truncate(3);
        for a in &mut self.actions {
            cut(a, 64);
        }
        Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifKey {
    pub key: String,
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
                charging: true,
                watts: 0,
                fast: false,
                full_in: -1,
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
    fn notifications_are_capped() {
        let n = PhoneNotification {
            key: "k".into(),
            pkg: "p".into(),
            app: "Chat\n".into(),
            title: "é".repeat(400),
            text: String::new(),
            time: 0,
            icon: "x".repeat(70_000),
            reply: true,
            actions: vec!["a".into(); 5],
            silent: false,
        };
        let s = n.clone().sanitized().unwrap();
        assert!(s.title.len() <= 512 && s.title.chars().all(|c| c == 'é'));
        assert_eq!(s.app, "Chat");
        assert!(s.icon.is_empty());
        assert_eq!(s.actions.len(), 3);
        let bad = PhoneNotification {
            key: String::new(),
            ..n
        };
        assert!(bad.sanitized().is_none());
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Pixel\u{0} 8\n "), "Pixel 8");
        assert_eq!(clean_name("\u{7}"), "Unnamed device");
        assert_eq!(clean_name(&"x".repeat(100)).len(), 64);
    }
}
