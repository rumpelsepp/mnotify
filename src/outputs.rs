use std::collections::BTreeMap;

use matrix_sdk::sync::UnreadNotificationsCount;
use serde::Serialize;

use matrix_sdk::sync::SyncResponse as BaseSyncResponse;
use matrix_sdk::{
    deserialized_responses::RawAnySyncOrStrippedTimelineEvent,
    ruma::{
        OwnedRoomId,
        events::{AnyGlobalAccountDataEvent, AnyToDeviceEvent, presence::PresenceEvent},
        push::Action,
        serde::Raw,
    },
};

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

#[derive(Serialize)]
pub(crate) struct Notification {
    pub(crate) actions: Vec<Action>,
    pub(crate) event: RawAnySyncOrStrippedTimelineEvent,
}

impl From<matrix_sdk::sync::Notification> for Notification {
    fn from(n: matrix_sdk::sync::Notification) -> Self {
        Self {
            actions: n.actions,
            event: n.event,
        }
    }
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
            to_device_events: value.to_device.iter().map(|e| e.to_raw()).collect(),
            notifications: value
                .notifications
                .into_iter()
                .map(|(room_id, ns)| (room_id, ns.into_iter().map(Notification::from).collect()))
                .collect(),
        }
    }
}
