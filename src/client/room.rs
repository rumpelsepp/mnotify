use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail};
use matrix_sdk::attachment::AttachmentConfig;
use matrix_sdk::room::{self, Messages, MessagesOptions, Room};
use matrix_sdk::ruma::events::room::message::{
    AddMentions, EmoteMessageEventContent, MessageType, RoomMessageEventContent,
};
use matrix_sdk::ruma::events::room::message::{ForwardThread, RoomMessageEvent};
use matrix_sdk::ruma::RoomId;
use matrix_sdk::ruma::{OwnedEventId, OwnedMxcUri};
use matrix_sdk::RoomMemberships;

impl super::Client {
    pub(crate) fn get_joined_room(
        &self,
        room_id: impl AsRef<RoomId>,
    ) -> anyhow::Result<room::Room> {
        self.inner
            .get_room(room_id.as_ref())
            .ok_or_else(|| anyhow!("no such room: {}", room_id.as_ref()))
    }

    pub(crate) async fn send_message_raw(
        &self,
        room_id: impl AsRef<RoomId>,
        content: RoomMessageEventContent,
    ) -> anyhow::Result<()> {
        let room = self.get_joined_room(room_id)?;
        room.send(content).await?;
        Ok(())
    }

    pub(crate) async fn send_message(
        &self,
        room: impl AsRef<RoomId>,
        body: &str,
        markdown: bool,
    ) -> anyhow::Result<()> {
        let content = if markdown {
            RoomMessageEventContent::text_markdown(body)
        } else {
            RoomMessageEventContent::text_plain(body)
        };
        self.send_message_raw(room, content).await
    }

    pub(crate) async fn send_message_reply(
        &self,
        room_id: impl AsRef<RoomId>,
        event_id: &OwnedEventId,
        body: &str,
        markdown: bool,
    ) -> anyhow::Result<()> {
        let room = self.get_joined_room(&room_id)?;
        let timeline_event = room.event(event_id).await?;
        let event_content = timeline_event.event.deserialize_as::<RoomMessageEvent>()?;
        let original_message = event_content.as_original().unwrap();

        let content = if markdown {
            RoomMessageEventContent::text_markdown(body)
        } else {
            RoomMessageEventContent::text_plain(body)
        }
        .make_reply_to(original_message, ForwardThread::Yes, AddMentions::No);

        self.send_message_raw(room_id, content).await
    }

    pub(crate) async fn send_notice(
        &self,
        room_id: impl AsRef<RoomId>,
        body: &str,
        markdown: bool,
    ) -> anyhow::Result<()> {
        let event = if markdown {
            RoomMessageEventContent::notice_markdown(body)
        } else {
            RoomMessageEventContent::notice_plain(body)
        };
        self.send_message_raw(room_id, event).await
    }

    pub(crate) async fn send_emote(
        &self,
        room_id: impl AsRef<RoomId>,
        body: &str,
        markdown: bool,
    ) -> anyhow::Result<()> {
        let content = if markdown {
            EmoteMessageEventContent::markdown(body)
        } else {
            EmoteMessageEventContent::plain(body)
        };
        let msgtype = MessageType::Emote(content);
        let content = RoomMessageEventContent::new(msgtype);
        self.send_message_raw(room_id, content).await
    }

    pub(crate) async fn send_attachment(
        &self,
        room_id: impl AsRef<RoomId>,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        let path = path.as_ref();
        let Some(file_name) = path.file_name().map(|s| s.to_str().unwrap()) else {
            bail!("invalid file: {:?}", path);
        };

        let room = self.get_joined_room(room_id)?;
        let data = fs::read(path)?;
        let config = AttachmentConfig::default().generate_thumbnail(None);
        let content_type = crate::mime::guess_mime(path)?;

        room.send_attachment(file_name, &content_type, data, config)
            .await?;
        Ok(())
    }

    pub(crate) fn mxc_to_http(&self, mxc: OwnedMxcUri) -> String {
        if !mxc.is_valid() {
            return String::from("");
        }
        format!(
            "{}_matrix/media/v3/thumbnail/{}/{}?width=50&height=50&method=scale",
            self.inner.homeserver().as_str(),
            mxc.server_name().unwrap(),
            mxc.media_id().unwrap(),
        )
    }

    pub(crate) async fn query_room(&self, room: Room) -> anyhow::Result<crate::outputs::Room> {
        let mut members_out = Vec::new();
        for member in room.members(RoomMemberships::empty()).await? {
            let avatar = match member.avatar_url() {
                Some(uri) => self.mxc_to_http(OwnedMxcUri::from(uri)),
                None => String::from(""),
            };
            members_out.push(crate::outputs::RoomMember {
                avatar,
                name: member.name().to_string(),
                display_name: member.display_name().map(|s| s.to_string()),
                user_id: member.user_id().to_string(),
            })
        }

        let mut opts = MessagesOptions::backward();
        opts.limit = (1).try_into().unwrap();

        let room_out = crate::outputs::Room {
            name: room.name(),
            topic: room.topic(),
            display_name: room.display_name().await?.to_string(),
            room_id: room.room_id().to_string(),
            is_encrypted: room.is_encrypted().await?,
            is_direct: room.is_direct().await?,
            is_tombstoned: room.is_tombstoned(),
            is_public: room.is_public(),
            is_space: room.is_space(),
            history_visibility: room.history_visibility().to_string(),
            guest_access: room.guest_access().to_string(),
            avatar: match room.avatar_url() {
                Some(url) => self.mxc_to_http(url.clone()),
                None => String::from(""),
            },
            matrix_uri: room.matrix_permalink(false).await?.to_string(),
            matrix_to_uri: room.matrix_to_permalink().await?.to_string(),
            unread_notifications: room.unread_notification_counts(),
            members: Some(members_out),
        };

        Ok(room_out)
    }

    pub(crate) async fn messages(
        &self,
        room_id: impl AsRef<RoomId>,
        limit: u64,
    ) -> anyhow::Result<Messages> {
        let room = self.get_joined_room(room_id)?;
        let mut options = MessagesOptions::backward();
        options.limit = limit.try_into()?;
        room.messages(options).await.map_err(|e| anyhow!(e))
    }
}
