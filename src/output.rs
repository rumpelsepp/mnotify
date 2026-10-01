//! What commands print on stdout: human-readable by default, the JSON that
//! scripts rely on with `--json`.

use comfy_table::{ContentArrangement, Table, presets};
use matrix_sdk::deserialized_responses::{EncryptionInfo, VerificationLevel, VerificationState};
use matrix_sdk::sync::UnreadNotificationsCount;
use serde::Serialize;
use serde::ser::SerializeMap;
use serde_json::Value;
use serde_json::value::RawValue;

/// Something a command prints.
pub(crate) trait Output: Serialize {
    /// The human-readable form.
    fn human(&self) -> String;
}

pub(crate) fn print(json: bool, out: &impl Output) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string(out)?);
    } else {
        println!("{}", out.human());
    }
    Ok(())
}

fn table() -> Table {
    let mut table = Table::new();
    table
        .load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table
}

fn key_value_table<'a>(rows: impl IntoIterator<Item = (&'a str, String)>) -> Table {
    let mut table = table();
    for (key, value) in rows {
        table.add_row(vec![key.to_owned(), value]);
    }
    table
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// A small result: a JSON object, shown as a key/value table, or as just
/// its headline field if it has one (e.g. the event ID of a sent message,
/// so `id=$(mn send …)` works without a JSON parser).
pub(crate) struct Record {
    fields: Vec<(&'static str, Value)>,
    headline: Option<&'static str>,
}

impl Record {
    pub(crate) fn new() -> Self {
        Self {
            fields: Vec::new(),
            headline: None,
        }
    }

    pub(crate) fn field(mut self, key: &'static str, value: impl Serialize) -> Self {
        self.fields
            .push((key, serde_json::to_value(value).unwrap_or(Value::Null)));
        self
    }

    pub(crate) fn headline(mut self, key: &'static str) -> Self {
        self.headline = Some(key);
        self
    }
}

impl Serialize for Record {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.fields.len()))?;
        for (key, value) in &self.fields {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl Output for Record {
    fn human(&self) -> String {
        let text = |value: &Value| match value {
            Value::String(s) => s.clone(),
            Value::Bool(b) => yes_no(*b).to_owned(),
            Value::Null => "-".to_owned(),
            other => other.to_string(),
        };
        match self.headline {
            Some(key) => self
                .fields
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| text(v))
                .unwrap_or_default(),
            // JSON keys read better with spaces: `user_id` -> `user id`.
            None => {
                let mut table = table();
                for (key, value) in &self.fields {
                    table.add_row(vec![key.replace('_', " "), text(value)]);
                }
                table.to_string()
            }
        }
    }
}

#[derive(Serialize)]
pub(crate) struct Room {
    pub(crate) name: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) display_name: String,
    pub(crate) room_id: String,
    pub(crate) guest_access: String,
    pub(crate) is_encrypted: bool,
    pub(crate) is_direct: bool,
    pub(crate) is_tombstoned: bool,
    pub(crate) is_public: bool,
    pub(crate) is_space: bool,
    pub(crate) history_visibility: String,
    /// `mxc://` URI of the room avatar. Media is authenticated (MSC3916), so
    /// fetch it through an authenticated client, not a plain HTTP GET.
    pub(crate) avatar: Option<String>,
    pub(crate) matrix_uri: String,
    pub(crate) matrix_to_uri: String,
    pub(crate) unread_notifications: UnreadNotificationsCount,
    pub(crate) members: Vec<RoomMember>,
}

#[derive(Serialize)]
pub(crate) struct RoomMember {
    pub(crate) name: String,
    pub(crate) display_name: Option<String>,
    pub(crate) user_id: String,
    /// `mxc://` URI of the member avatar (see `Room::avatar`).
    pub(crate) avatar: Option<String>,
}

impl Output for Room {
    fn human(&self) -> String {
        let details = key_value_table([
            ("name", self.display_name.clone()),
            ("room id", self.room_id.clone()),
            ("topic", self.topic.clone().unwrap_or_else(|| "-".into())),
            ("encrypted", yes_no(self.is_encrypted).into()),
            ("direct", yes_no(self.is_direct).into()),
            ("public", yes_no(self.is_public).into()),
            (
                "unread",
                self.unread_notifications.notification_count.to_string(),
            ),
            ("link", self.matrix_to_uri.clone()),
        ]);
        let mut members = table();
        members.set_header(vec!["MEMBER", "DISPLAY NAME"]);
        for member in &self.members {
            members.add_row(vec![
                member.user_id.clone(),
                member.display_name.clone().unwrap_or_default(),
            ]);
        }
        format!("{details}\n\n{members}")
    }
}

/// The room list of `mn rooms`; one line per room.
#[derive(Serialize)]
#[serde(transparent)]
pub(crate) struct Rooms(pub(crate) Vec<Room>);

impl Output for Rooms {
    fn human(&self) -> String {
        let mut table = table();
        table.set_header(vec!["NAME", "ROOM ID", "MEMBERS", "ENCRYPTED", "UNREAD"]);
        for room in &self.0 {
            table.add_row(vec![
                room.display_name.clone(),
                room.room_id.clone(),
                room.members.len().to_string(),
                yes_no(room.is_encrypted).into(),
                room.unread_notifications.notification_count.to_string(),
            ]);
        }
        table.to_string()
    }
}

/// Whether an encrypted event's sender is not proven: its room key has no
/// known sending device, as for keys from the key backup. Element shows a
/// grey shield ("authenticity cannot be guaranteed") for these.
pub(crate) fn sender_unproven(info: Option<&EncryptionInfo>) -> bool {
    info.is_some_and(|info| {
        !matches!(
            info.verification_state,
            VerificationState::Verified
                | VerificationState::Unverified(VerificationLevel::UnverifiedIdentity)
        )
    })
}

/// A timeline event as printed: the raw event (also its JSON form), plus
/// whether its sender is unproven (see `sender_unproven`).
pub(crate) struct Event {
    pub(crate) raw: Box<RawValue>,
    pub(crate) unproven: bool,
}

/// Timeline events of `mn messages`, oldest first.
pub(crate) struct Events(pub(crate) Vec<Event>);

impl Serialize for Events {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(|event| &event.raw))
    }
}

impl Output for Events {
    fn human(&self) -> String {
        let lines: Vec<String> = self
            .0
            .iter()
            .map(|event| event_line(&event.raw, event.unproven))
            .collect();
        lines.join("\n")
    }
}

/// One timeline event as a line: local time, sender, and what happened;
/// `[unverified]` if the sender is unproven. Shared by `mn messages` and
/// `mn sync`.
pub(crate) fn event_line(event: &RawValue, unproven: bool) -> String {
    let Ok(event) = serde_json::from_str::<Value>(event.get()) else {
        return "[unreadable event]".to_owned();
    };
    let time = event["origin_server_ts"]
        .as_i64()
        .and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();
    let sender = event["sender"].as_str().unwrap_or("?");
    let content = &event["content"];
    let body = content["body"].as_str().unwrap_or_default();

    let text = match (event["type"].as_str(), content["msgtype"].as_str()) {
        (Some("m.room.message"), Some("m.emote")) => format!("* {body}"),
        (Some("m.room.message"), Some("m.image" | "m.file" | "m.audio" | "m.video")) => {
            format!("[file] {body}")
        }
        (Some("m.room.message"), _) if content.get("body").is_some() => body.to_owned(),
        (Some("m.room.message"), _) => "[deleted]".to_owned(),
        (Some("m.room.encrypted"), _) => "[unable to decrypt]".to_owned(),
        (Some("m.room.member"), _) => format!(
            "[{} {}]",
            content["membership"].as_str().unwrap_or("member"),
            event["state_key"].as_str().unwrap_or("?"),
        ),
        (Some(other), _) => format!("[{other}]"),
        (None, _) => "[unknown event]".to_owned(),
    };
    let thread = if content["m.relates_to"]["rel_type"] == "m.thread" {
        "↳ "
    } else {
        ""
    };
    let unverified = if unproven { "[unverified] " } else { "" };

    let prefix = format!("{time}  {sender}  {thread}{unverified}");
    // Continuation lines of multi-line messages line up under the first one.
    let indent = " ".repeat(prefix.chars().count());
    format!("{prefix}{}", text.replace('\n', &format!("\n{indent}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(event: serde_json::Value) -> String {
        let raw = RawValue::from_string(event.to_string()).unwrap();
        event_line(&raw, false)
    }

    fn message(content: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "type": "m.room.message",
            "sender": "@bot:example.org",
            "origin_server_ts": 0,
            "content": content,
        })
    }

    #[test]
    fn event_lines() {
        let text = line(message(
            serde_json::json!({"msgtype": "m.text", "body": "hi"}),
        ));
        assert!(text.ends_with("@bot:example.org  hi"), "{text}");

        let emote = line(message(
            serde_json::json!({"msgtype": "m.emote", "body": "waves"}),
        ));
        assert!(emote.ends_with("  * waves"), "{emote}");

        let file = line(message(
            serde_json::json!({"msgtype": "m.file", "body": "a.log"}),
        ));
        assert!(file.ends_with("  [file] a.log"), "{file}");

        let thread = line(message(serde_json::json!({
            "msgtype": "m.text",
            "body": "step 2",
            "m.relates_to": {"rel_type": "m.thread", "event_id": "$root"},
        })));
        assert!(thread.ends_with("  ↳ step 2"), "{thread}");

        let mut encrypted = message(serde_json::json!({}));
        encrypted["type"] = "m.room.encrypted".into();
        assert!(line(encrypted).ends_with("[unable to decrypt]"));

        let raw = RawValue::from_string(
            message(serde_json::json!({"msgtype": "m.text", "body": "old"})).to_string(),
        )
        .unwrap();
        let unverified = event_line(&raw, true);
        assert!(unverified.ends_with("  [unverified] old"), "{unverified}");

        let multi = line(message(
            serde_json::json!({"msgtype": "m.text", "body": "a\nb"}),
        ));
        let (first, second) = multi.split_once('\n').unwrap();
        assert_eq!(first.len() - 1, second.len() - 1, "continuation is aligned");
        assert!(second.trim_start() == "b");
    }

    #[test]
    fn records() {
        let sent = Record::new()
            .field("room_id", "!r:example.org")
            .field("event_id", "$e")
            .headline("event_id");
        assert_eq!(sent.human(), "$e");
        assert_eq!(
            serde_json::to_string(&sent).unwrap(),
            r#"{"room_id":"!r:example.org","event_id":"$e"}"#
        );

        let me = Record::new()
            .field("is_guest", false)
            .field("token", None::<String>);
        let human = me.human();
        assert!(human.contains("is guest") && human.contains("no") && human.contains('-'));
    }
}
