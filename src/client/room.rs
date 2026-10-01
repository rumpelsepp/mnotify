use std::fs;
use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, anyhow, bail};
use image::{GenericImageView, ImageFormat};
use matrix_sdk::attachment::{AttachmentConfig, AttachmentInfo, BaseImageInfo, Thumbnail};
use matrix_sdk::room::{Messages, MessagesOptions, Room};
use matrix_sdk::ruma::events::room::message::{
    AddMentions, ForwardThread, RoomMessageEvent, RoomMessageEventContent,
};
use matrix_sdk::ruma::{EventId, OwnedEventId, OwnedRoomId, RoomId, RoomOrAliasId, UInt};
use matrix_sdk::{RoomMemberships, RoomState};

/// Which flavour of `m.room.message` to send.
#[derive(Debug, Clone, Copy)]
pub(crate) enum TextKind {
    Text,
    Notice,
    Emote,
}

/// Longest edge of a generated thumbnail, in pixels.
const THUMBNAIL_SIZE: u32 = 800;

/// Best-effort image dimensions plus a downscaled thumbnail, so clients can
/// render an inline preview without downloading the full image first. Any
/// decode failure falls back to a plain upload.
fn image_attachment_config(data: &[u8]) -> AttachmentConfig {
    let Ok(image) = image::load_from_memory(data) else {
        return AttachmentConfig::new();
    };
    let (width, height) = image.dimensions();

    let mut config = AttachmentConfig::new().info(AttachmentInfo::Image(BaseImageInfo {
        width: UInt::new(width.into()),
        height: UInt::new(height.into()),
        size: UInt::new(data.len() as u64),
        blurhash: None,
        is_animated: None,
    }));

    if width > THUMBNAIL_SIZE || height > THUMBNAIL_SIZE {
        let thumbnail = image.thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE);
        let (tw, th) = thumbnail.dimensions();
        // JPEG unless the image has an alpha channel, which JPEG cannot keep.
        let (format, content_type) = if image.color().has_alpha() {
            (ImageFormat::Png, mime::IMAGE_PNG)
        } else {
            (ImageFormat::Jpeg, mime::IMAGE_JPEG)
        };
        let mut buf = Cursor::new(Vec::new());
        if thumbnail.write_to(&mut buf, format).is_ok() {
            let bytes = buf.into_inner();
            config = config.thumbnail(Some(Thumbnail {
                size: UInt::new(bytes.len() as u64).unwrap_or_default(),
                data: bytes,
                content_type,
                width: UInt::new(tw.into()).unwrap_or_default(),
                height: UInt::new(th.into()).unwrap_or_default(),
            }));
        }
    }

    config
}

impl super::Client {
    /// Resolve a room ID or alias to a room ID. An alias costs one request.
    pub(crate) async fn resolve_room_id(
        &self,
        room: &RoomOrAliasId,
    ) -> anyhow::Result<OwnedRoomId> {
        match <&RoomId>::try_from(room) {
            Ok(room_id) => Ok(room_id.to_owned()),
            Err(alias) => Ok(self
                .inner
                .resolve_room_alias(alias)
                .await
                .with_context(|| format!("could not resolve room alias {alias}"))?
                .room_id),
        }
    }

    /// Look up a room we are a member of, with a hint on what to do if we are
    /// not (yet).
    pub(crate) async fn joined_room(&self, room: &RoomOrAliasId) -> anyhow::Result<Room> {
        let room_id = self.resolve_room_id(room).await?;
        let Some(joined) = self.inner.get_room(&room_id) else {
            bail!("not a member of {room}; join it first: mn join '{room}'");
        };
        match joined.state() {
            RoomState::Joined => Ok(joined),
            RoomState::Invited => bail!("{room} is a pending invite; accept it: mn join '{room}'"),
            state => bail!("not a member of {room} (state: {state:?}); join it: mn join '{room}'"),
        }
    }

    /// Join a room by ID or alias; this also accepts a pending invite.
    pub(crate) async fn join(&self, room: &RoomOrAliasId) -> anyhow::Result<Room> {
        Ok(self.inner.join_room_by_id_or_alias(room, &[]).await?)
    }

    pub(crate) async fn send_text(
        &self,
        room: &Room,
        body: &str,
        kind: TextKind,
        markdown: bool,
    ) -> anyhow::Result<OwnedEventId> {
        let content = match (kind, markdown) {
            (TextKind::Text, false) => RoomMessageEventContent::text_plain(body),
            (TextKind::Text, true) => RoomMessageEventContent::text_markdown(body),
            (TextKind::Notice, false) => RoomMessageEventContent::notice_plain(body),
            (TextKind::Notice, true) => RoomMessageEventContent::notice_markdown(body),
            (TextKind::Emote, false) => RoomMessageEventContent::emote_plain(body),
            (TextKind::Emote, true) => RoomMessageEventContent::emote_markdown(body),
        };
        Ok(room.send(content).await?.response.event_id)
    }

    pub(crate) async fn send_message_reply(
        &self,
        room: &Room,
        event_id: &EventId,
        body: &str,
        markdown: bool,
    ) -> anyhow::Result<OwnedEventId> {
        let replied_to = room
            .event(event_id, None)
            .await?
            .raw()
            .deserialize_as_unchecked::<RoomMessageEvent>()?;
        let original = replied_to
            .as_original()
            .ok_or_else(|| anyhow!("cannot reply to a redacted event"))?;

        let content = if markdown {
            RoomMessageEventContent::text_markdown(body)
        } else {
            RoomMessageEventContent::text_plain(body)
        }
        .make_reply_to(original, ForwardThread::Yes, AddMentions::No);

        Ok(room.send(content).await?.response.event_id)
    }

    pub(crate) async fn send_attachment(
        &self,
        room: &Room,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<OwnedEventId> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid file name: {path:?}"))?;
        let data = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
        let content_type = mime_guess::from_path(path).first_or_octet_stream();

        let config = if content_type.type_() == mime::IMAGE {
            image_attachment_config(&data)
        } else {
            AttachmentConfig::new()
        };

        Ok(room
            .send_attachment(file_name, &content_type, data, config)
            .await?
            .event_id)
    }

    pub(crate) async fn query_room(&self, room: Room) -> anyhow::Result<crate::outputs::Room> {
        let mut members = Vec::new();
        for member in room.members(RoomMemberships::empty()).await? {
            members.push(crate::outputs::RoomMember {
                avatar: member.avatar_url().map(ToString::to_string),
                name: member.name().to_owned(),
                display_name: member.display_name().map(ToOwned::to_owned),
                user_id: member.user_id().to_string(),
            });
        }

        members.sort_by(|a, b| a.user_id.cmp(&b.user_id));

        Ok(crate::outputs::Room {
            name: room.name(),
            topic: room.topic(),
            display_name: room.display_name().await?.to_string(),
            room_id: room.room_id().to_string(),
            is_encrypted: room.latest_encryption_state().await?.is_encrypted(),
            is_direct: room.is_direct().await?,
            is_tombstoned: room.is_tombstoned(),
            is_public: room.is_public().unwrap_or(false),
            is_space: room.is_space(),
            history_visibility: room.history_visibility_or_default().to_string(),
            guest_access: room.guest_access().to_string(),
            avatar: room.avatar_url().map(|uri| uri.to_string()),
            matrix_uri: room.matrix_permalink(false).await?.to_string(),
            matrix_to_uri: room.matrix_to_permalink().await?.to_string(),
            unread_notifications: room.unread_notification_counts(),
            members,
        })
    }

    pub(crate) async fn messages(&self, room: &Room, limit: u64) -> anyhow::Result<Messages> {
        let mut options = MessagesOptions::backward();
        options.limit = limit.try_into()?;
        Ok(room.messages(options).await?)
    }
}
