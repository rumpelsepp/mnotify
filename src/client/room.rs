use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, anyhow, bail};
use image::{GenericImageView, ImageFormat};
use matrix_sdk::attachment::{
    AttachmentConfig, AttachmentInfo, BaseAudioInfo, BaseFileInfo, BaseImageInfo, BaseVideoInfo,
    Thumbnail,
};
use matrix_sdk::deserialized_responses::{TimelineEvent, TimelineEventKind, UnableToDecryptReason};
use matrix_sdk::room::reply::{EnforceThread, Reply};
use matrix_sdk::room::{IncludeRelations, MessagesOptions, RelationsOptions, Room};
use matrix_sdk::ruma::api::client::room::create_room::{self, v3::RoomPreset};
use matrix_sdk::ruma::api::client::typing::create_typing_event::{self, v3::Typing};
use matrix_sdk::ruma::events::relation::RelationType;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::events::room::message::{
    AddMentions, ReplyWithinThread, RoomMessageEventContentWithoutRelation,
};
use matrix_sdk::ruma::events::{InitialStateEvent, Mentions};
use matrix_sdk::ruma::{
    EventId, OwnedEventId, OwnedRoomId, OwnedUserId, RoomId, RoomOrAliasId, UInt,
};
use matrix_sdk::{RoomMemberships, RoomState};
use tracing::warn;

/// What `mn room create` asks the homeserver for.
pub(crate) struct NewRoom {
    pub(crate) name: Option<String>,
    pub(crate) topic: Option<String>,
    /// Local part of the alias, e.g. `ops` for `#ops:example.org`.
    pub(crate) alias: Option<String>,
    pub(crate) invite: Vec<OwnedUserId>,
    /// Anyone can join (`public_chat`) instead of invite only (`private_chat`).
    pub(crate) public: bool,
    pub(crate) encrypted: bool,
}

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
        return AttachmentConfig::new().info(AttachmentInfo::Image(BaseImageInfo {
            size: UInt::new(data.len() as u64),
            ..Default::default()
        }));
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
            bail!("not a member of {room}; join it first: mn room join '{room}'");
        };
        match joined.state() {
            RoomState::Joined => Ok(joined),
            RoomState::Invited => {
                bail!("{room} is a pending invite; accept it: mn room join '{room}'")
            }
            state => {
                bail!("not a member of {room} (state: {state:?}); join it: mn room join '{room}'")
            }
        }
    }

    /// Create a room we are the only member of; `invite` is invited right away.
    pub(crate) async fn create_room(&self, opts: NewRoom) -> anyhow::Result<Room> {
        let mut request = create_room::v3::Request::new();
        request.name = opts.name;
        request.topic = opts.topic;
        request.room_alias_name = opts.alias;
        request.invite = opts.invite;
        request.preset = Some(if opts.public {
            RoomPreset::PublicChat
        } else {
            RoomPreset::PrivateChat
        });
        if opts.encrypted {
            request.initial_state = vec![
                InitialStateEvent::with_empty_state_key(
                    RoomEncryptionEventContent::with_recommended_defaults(),
                )
                .to_raw_any(),
            ];
        }
        Ok(self.inner.create_room(request).await?)
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
        addressing: Addressing,
    ) -> anyhow::Result<OwnedEventId> {
        type Content = RoomMessageEventContentWithoutRelation;
        let content = match (kind, markdown) {
            (TextKind::Text, false) => Content::text_plain(body),
            (TextKind::Text, true) => Content::text_markdown(body),
            (TextKind::Notice, false) => Content::notice_plain(body),
            (TextKind::Notice, true) => Content::notice_markdown(body),
            (TextKind::Emote, false) => Content::emote_plain(body),
            (TextKind::Emote, true) => Content::emote_markdown(body),
        };
        let content = content.add_mentions(addressing.mentions);
        let content = match addressing.relation {
            // Boxed: the SDK future is deep enough to hit rustc's query depth
            // limit when inlined into this one.
            Some(relation) => Box::pin(room.make_reply_event(content, relation.into())).await?,
            None => content.with_relation(None),
        };
        Ok(room.send(content).await?.response.event_id)
    }

    pub(crate) async fn send_attachment(
        &self,
        room: &Room,
        path: impl AsRef<Path>,
        addressing: Addressing,
    ) -> anyhow::Result<OwnedEventId> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid file name: {path:?}"))?;
        let data = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
        let content_type = mime_guess::from_path(path).first_or_octet_stream();

        // The SDK leaves out the size unless it is given, and clients then
        // cannot say how large a file is before downloading it.
        let size = UInt::new(data.len() as u64);
        let mut config = match content_type.type_() {
            mime::IMAGE => image_attachment_config(&data),
            mime::AUDIO => AttachmentConfig::new().info(AttachmentInfo::Audio(BaseAudioInfo {
                size,
                ..Default::default()
            })),
            mime::VIDEO => AttachmentConfig::new().info(AttachmentInfo::Video(BaseVideoInfo {
                size,
                ..Default::default()
            })),
            _ => AttachmentConfig::new().info(AttachmentInfo::File(BaseFileInfo { size })),
        };
        config.reply = addressing.relation.map(Into::into);
        config.mentions = Some(addressing.mentions);

        Ok(room
            .send_attachment(file_name, &content_type, data, config)
            .await?
            .event_id)
    }

    /// Show or hide the typing indicator. Not `Room::typing_notice` alone:
    /// it only sends "stopped typing" if this process announced typing
    /// before, which a fresh `mn typing --disable` never did.
    pub(crate) async fn set_typing(&self, room: &Room, typing: bool) -> anyhow::Result<()> {
        if typing {
            room.typing_notice(true).await?;
        } else {
            let request = create_typing_event::v3::Request::new(
                self.user_id.clone(),
                room.room_id().to_owned(),
                Typing::No,
            );
            self.inner.send(request).await?;
        }
        Ok(())
    }

    pub(crate) async fn query_room(&self, room: Room) -> anyhow::Result<crate::output::Room> {
        let mut members = Vec::new();
        for member in room.members(RoomMemberships::empty()).await? {
            members.push(crate::output::RoomMember {
                avatar: member.avatar_url().map(ToString::to_string),
                name: member.name().to_owned(),
                display_name: member.display_name().map(ToOwned::to_owned),
                user_id: member.user_id().to_string(),
            });
        }

        members.sort_by(|a, b| a.user_id.cmp(&b.user_id));

        Ok(crate::output::Room {
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

    /// `@room` only notifies if our power level allows it; fail instead of
    /// sending a message that silently notifies no one.
    pub(crate) async fn ensure_can_mention_room(&self, room: &Room) -> anyhow::Result<()> {
        let me = room
            .get_member(&self.user_id)
            .await?
            .context("not a member of the room")?;
        anyhow::ensure!(
            me.can_trigger_room_notification(),
            "{} may not notify the whole room (@room) in {}; raise its power level",
            self.user_id,
            room.room_id(),
        );
        Ok(())
    }

    /// The latest `limit` events of the room, oldest first.
    pub(crate) async fn messages(
        &self,
        room: &Room,
        limit: u64,
    ) -> anyhow::Result<Vec<TimelineEvent>> {
        let mut options = MessagesOptions::backward();
        options.limit = limit.try_into()?;
        let mut events = room.messages(options).await?.chunk;
        self.decrypt_from_backup(room, &mut events).await;
        events.reverse();
        Ok(events)
    }

    /// A thread: its root and the latest `limit` events in it, oldest first.
    pub(crate) async fn thread(
        &self,
        room: &Room,
        root: &EventId,
        limit: u64,
    ) -> anyhow::Result<Vec<TimelineEvent>> {
        let mut options = RelationsOptions {
            include_relations: IncludeRelations::RelationsOfType(RelationType::Thread),
            ..Default::default()
        };
        options.limit = Some(limit.try_into()?);
        let mut events = room.relations(root.to_owned(), options).await?.chunk;
        events.push(room.event(root, None).await?);
        self.decrypt_from_backup(room, &mut events).await;
        events.reverse();
        Ok(events)
    }

    /// Decrypt events this device never received the room key for (e.g. sent
    /// before it logged in) with keys from the server-side key backup. The SDK
    /// does not fetch them on its own (`BackupDownloadStrategy::Manual`), and
    /// its background download on failure would come too late for a one-shot
    /// command. Without an enabled backup, the events stay undecryptable.
    async fn decrypt_from_backup(&self, room: &Room, events: &mut [TimelineEvent]) {
        let backups = self.inner.encryption().backups();
        let mut downloaded = HashSet::new();
        for event in events {
            let TimelineEventKind::UnableToDecrypt {
                event: raw,
                utd_info,
            } = &event.kind
            else {
                continue;
            };
            let (
                UnableToDecryptReason::MissingMegolmSession { .. }
                | UnableToDecryptReason::UnknownMegolmMessageIndex,
                Some(session_id),
            ) = (&utd_info.reason, &utd_info.session_id)
            else {
                continue;
            };
            if !downloaded.contains(session_id) {
                match backups.download_room_key(room.room_id(), session_id).await {
                    Ok(true) => {}
                    Ok(false) => return, // no backup enabled on this device
                    Err(e) => {
                        warn!("cannot download room key {session_id} from the backup: {e}");
                        continue;
                    }
                }
                downloaded.insert(session_id.clone());
            }
            match room.decrypt_event(raw.cast_ref_unchecked(), None).await {
                Ok(decrypted) => *event = decrypted,
                Err(e) => warn!("cannot decrypt event: {e}"),
            }
        }
    }
}

/// Where a new message goes and whom it notifies.
///
/// `mentions` is always sent, empty if nobody is mentioned: that opts the
/// message out of the legacy push rules that match names in the body, so a
/// log line containing someone's name does not notify them.
#[derive(Default)]
pub(crate) struct Addressing {
    pub(crate) relation: Option<Relation>,
    pub(crate) mentions: Mentions,
}

/// How a new message relates to an existing one.
pub(crate) enum Relation {
    /// A reply to the event; stays in its thread if it is in one.
    Reply(OwnedEventId),
    /// A message in the thread of the event, starting one if there is none.
    Thread(OwnedEventId),
}

impl From<Relation> for Reply {
    fn from(relation: Relation) -> Self {
        let (event_id, enforce_thread) = match relation {
            Relation::Reply(event_id) => (event_id, EnforceThread::MaybeThreaded),
            Relation::Thread(event_id) => {
                (event_id, EnforceThread::Threaded(ReplyWithinThread::No))
            }
        };
        Reply {
            event_id,
            enforce_thread,
            add_mentions: AddMentions::No,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat};
    use matrix_sdk::attachment::AttachmentInfo;

    use super::{THUMBNAIL_SIZE, image_attachment_config};

    fn png(image: DynamicImage) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        image.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    fn size(info: Option<&AttachmentInfo>) -> (u64, u64) {
        let Some(AttachmentInfo::Image(info)) = info else {
            panic!("no image info");
        };
        (info.width.unwrap().into(), info.height.unwrap().into())
    }

    #[test]
    fn small_images_get_no_thumbnail() {
        let config = image_attachment_config(&png(DynamicImage::new_rgb8(40, 20)));
        assert_eq!(size(config.info.as_ref()), (40, 20));
        assert!(config.thumbnail.is_none());
    }

    #[test]
    fn large_images_get_a_thumbnail_within_the_bounds() {
        let config = image_attachment_config(&png(DynamicImage::new_rgb8(1600, 400)));
        assert_eq!(size(config.info.as_ref()), (1600, 400));

        let thumbnail = config.thumbnail.unwrap();
        assert_eq!(thumbnail.content_type, mime::IMAGE_JPEG);
        let (width, height): (u64, u64) = (thumbnail.width.into(), thumbnail.height.into());
        assert_eq!(
            (width, height),
            (THUMBNAIL_SIZE.into(), 200),
            "aspect ratio kept"
        );
        let decoded = image::load_from_memory_with_format(&thumbnail.data, ImageFormat::Jpeg);
        assert_eq!(decoded.unwrap().width(), THUMBNAIL_SIZE);
    }

    #[test]
    fn transparent_images_get_a_png_thumbnail() {
        let config = image_attachment_config(&png(DynamicImage::new_rgba8(400, 1000)));
        let thumbnail = config.thumbnail.unwrap();
        assert_eq!(thumbnail.content_type, mime::IMAGE_PNG);
        assert!(image::load_from_memory_with_format(&thumbnail.data, ImageFormat::Png).is_ok());
    }

    #[test]
    fn undecodable_images_get_only_their_size() {
        let config = image_attachment_config(b"not an image");
        assert!(config.thumbnail.is_none());
        let Some(AttachmentInfo::Image(info)) = config.info else {
            panic!("no image info");
        };
        assert_eq!(info.size, matrix_sdk::ruma::UInt::new(12));
        assert!(info.width.is_none() && info.height.is_none());
    }
}
