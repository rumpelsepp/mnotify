use std::collections::BTreeMap;

use matrix_sdk::ruma::api::client::sync::sync_events::UnreadNotificationsCount;
use matrix_sdk::sync::UnreadNotificationsCount as OtherUnreadNotificationsCount;
use serde::Serialize;

use matrix_sdk::sync::SyncResponse as BaseSyncResponse;
use matrix_sdk::{
    deserialized_responses::SyncTimelineEvent,
    ruma::{
        api::client::push::get_notifications::v3::Notification,
        events::{presence::PresenceEvent, AnyGlobalAccountDataEvent, AnyToDeviceEvent},
        serde::Raw,
        OwnedRoomId,
    },
};
use serde_json::value::RawValue;

#[derive(Serialize)]
pub(crate) struct SSRoom {
    pub(crate) name: Option<String>,
    pub(crate) room_id: String,
    pub(crate) is_direct: bool,
    pub(crate) avatar: String,
    pub(crate) unread_notifications: UnreadNotificationsCount,
    pub(crate) events: Vec<SyncTimelineEvent>,
    pub(crate) members: Vec<RoomMember>,
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
    pub(crate) avatar: String,
    pub(crate) matrix_uri: String,
    pub(crate) matrix_to_uri: String,
    pub(crate) unread_notifications: OtherUnreadNotificationsCount,
    pub(crate) members: Option<Vec<RoomMember>>,
    //pub(crate) latest_event: Option<SyncTimelineEvent>,
    // pub(crate) events: Vec<Box<RawValue>>,
}

#[derive(Serialize)]
pub(crate) struct RoomMember {
    pub(crate) name: String,
    pub(crate) display_name: Option<String>,
    pub(crate) user_id: String,
    pub(crate) avatar: String,
}

// https://matrix-org.github.io/matrix-rust-sdk/matrix_sdk/sync/struct.SyncResponse.html
#[derive(Serialize)]
pub(crate) struct SyncResponse {
    pub(crate) presence: Vec<Raw<PresenceEvent>>,
    pub(crate) account_data: Vec<Raw<AnyGlobalAccountDataEvent>>,
    pub(crate) to_device_events: Vec<Raw<AnyToDeviceEvent>>,
    pub(crate) notifications: BTreeMap<OwnedRoomId, Vec<Notification>>,
}

impl From<BaseSyncResponse> for SyncResponse {
    fn from(value: BaseSyncResponse) -> Self {
        Self {
            presence: value.presence,
            account_data: value.account_data,
            to_device_events: value.to_device,
            notifications: value.notifications,
        }
    }
}
