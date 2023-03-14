use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail};
use matrix_sdk::attachment::AttachmentConfig;
use matrix_sdk::room::{self, Messages, MessagesOptions, Room};
use matrix_sdk::ruma::events::room::message::{
    EmoteMessageEventContent, MessageType, RoomMessageEventContent,
};
use matrix_sdk::ruma::{EventId, RoomId};

impl super::Client {
    fn get_joined_room(&self, room: impl AsRef<RoomId>) -> anyhow::Result<room::Joined> {
        self.inner
            .get_joined_room(room.as_ref())
            .ok_or_else(|| anyhow!("no such room: {}", room.as_ref()))
    }

    pub(crate) async fn redact(
        &self,
        room: impl AsRef<RoomId>,
        event_id: &EventId,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        let room = self.get_joined_room(room)?;
        room.redact(event_id, reason, None).await?;
        Ok(())
    }

    pub(crate) async fn send_message_raw(
        &self,
        room: impl AsRef<RoomId>,
        content: RoomMessageEventContent,
    ) -> anyhow::Result<()> {
        let room = self.get_joined_room(room)?;
        room.send(content, None).await?;
        Ok(())
    }

    pub(crate) async fn send_message(
        &self,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let event = RoomMessageEventContent::text_plain(msg);
        self.send_message_raw(room, event).await
    }

    pub(crate) async fn send_message_md(
        &self,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let event = RoomMessageEventContent::text_markdown(msg);
        self.send_message_raw(room, event).await
    }

    pub(crate) async fn send_notice(
        &self,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let event = RoomMessageEventContent::notice_plain(msg);
        self.send_message_raw(room, event).await
    }

    pub(crate) async fn send_notice_md(
        &self,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let event = RoomMessageEventContent::notice_markdown(msg);
        self.send_message_raw(room, event).await
    }

    async fn send_emote_raw(
        &self,
        room: impl AsRef<RoomId>,
        content: EmoteMessageEventContent,
    ) -> anyhow::Result<()> {
        let msg = MessageType::Emote(content);
        let event = RoomMessageEventContent::new(msg);
        self.send_message_raw(room, event).await
    }

    pub(crate) async fn send_emote(
        &self,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let content = EmoteMessageEventContent::plain(msg);
        self.send_emote_raw(room, content).await
    }

    pub(crate) async fn send_emote_md(
        &self,
        room: impl AsRef<RoomId>,
        msg: &str,
    ) -> anyhow::Result<()> {
        let content = EmoteMessageEventContent::markdown(msg);
        self.send_emote_raw(room, content).await
    }

    pub(crate) async fn send_attachment(
        &self,
        room: impl AsRef<RoomId>,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        let path = path.as_ref();
        let Some(file_name) = path.file_name().map(|s|s.to_str().unwrap()) else {
            bail!("invalid file: {:?}", path);
        };
        let Some(extension) = path.extension().map(|s|s.to_str().unwrap()) else {
            bail!("invalid file extension: {:?}", path);
        };

        let room = self.get_joined_room(room)?;
        let data = fs::read(path)?;
        let config = AttachmentConfig::default().generate_thumbnail(None);
        let mime_type = crate::mime::guess_mime(extension);

        room.send_attachment(file_name, &mime_type, data, config)
            .await?;
        Ok(())
    }

    pub(crate) async fn send_typing(
        &self,
        room: impl AsRef<RoomId>,
        enabled: bool,
    ) -> anyhow::Result<()> {
        let room = self.get_joined_room(room)?;
        room.typing_notice(enabled).await?;
        Ok(())
    }

    pub(crate) async fn query_room(
        &self,
        room: Room,
        query_avatars: bool,
        query_members: bool,
    ) -> anyhow::Result<crate::outputs::Room> {
        let room_avatar = if query_avatars {
            room.avatar(matrix_sdk::media::MediaFormat::File).await?
        } else {
            None
        };

        let mut room_out = crate::outputs::Room {
            name: room.name(),
            topic: room.topic(),
            display_name: room.display_name().await?.to_string(),
            room_id: room.room_id().to_string(),
            is_encrypted: room.is_encrypted().await?,
            is_direct: room.is_direct(),
            is_tombstoned: room.is_tombstoned(),
            is_public: room.is_public(),
            is_space: room.is_space(),
            history_visibility: room.history_visibility().to_string(),
            guest_access: room.guest_access().to_string(),
            avatar: room_avatar,
            matrix_to_uri: room.matrix_to_permalink().await?.to_string(),
            unread_notifications: room.unread_notification_counts(),
            members: None,
        };

        if query_members {
            let mut members_out = vec![];
            for member in room.members().await? {
                let member_avatar = if query_avatars {
                    member.avatar(matrix_sdk::media::MediaFormat::File).await?
                } else {
                    None
                };

                members_out.push(crate::outputs::RoomMember {
                    avatar: member_avatar,
                    name: member.name().to_string(),
                    display_name: member.display_name().map(|s| s.to_string()),
                    user_id: member.user_id().to_string(),
                })
            }

            room_out.members = Some(members_out);
        }

        Ok(room_out)
    }

    pub(crate) async fn messages(
        &self,
        room: impl AsRef<RoomId>,
        limit: u64,
    ) -> anyhow::Result<Messages> {
        let room = self.get_joined_room(room)?;
        let mut options = MessagesOptions::backward();
        options.limit = limit.try_into()?;
        room.messages(options).await.map_err(|e| anyhow!(e))
    }
}
