use std::collections::BTreeMap;

use serde::Serialize;

use matrix_sdk::ruma::{
    api::client::{
        push::get_notifications::v3::Notification,
        sync::sync_events::{v3::Presence, DeviceLists},
    },
    events::{AnyGlobalAccountDataEvent, AnyToDeviceEvent},
    serde::Raw,
    DeviceKeyAlgorithm, OwnedRoomId,
};
use matrix_sdk::sync::SyncResponse as BaseSyncResponse;
use matrix_sdk::sync::UnreadNotificationsCount;
pub use matrix_sdk::sync::*;

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
    #[serde(with = "crate::base64")]
    pub(crate) avatar: Option<Vec<u8>>,
    pub(crate) matrix_uri: String,
    pub(crate) matrix_to_uri: String,
    pub(crate) unread_notifications: UnreadNotificationsCount,
    pub(crate) members: Option<Vec<RoomMember>>,
}

#[derive(Serialize)]
pub(crate) struct RoomMember {
    pub(crate) name: String,
    pub(crate) display_name: Option<String>,
    pub(crate) user_id: String,
    #[serde(with = "crate::base64")]
    pub(crate) avatar: Option<Vec<u8>>,
}

// https://matrix-org.github.io/matrix-rust-sdk/matrix_sdk/sync/struct.SyncResponse.html
#[derive(Serialize)]
pub(crate) struct SyncResponse {
    pub(crate) rooms: Rooms,
    pub(crate) presence: Presence,
    pub(crate) account_data: Vec<Raw<AnyGlobalAccountDataEvent>>,
    pub(crate) to_device_events: Vec<Raw<AnyToDeviceEvent>>,
    pub(crate) device_lists: DeviceLists,
    pub(crate) device_one_time_keys_count: BTreeMap<DeviceKeyAlgorithm, u64>,
    pub(crate) notifications: BTreeMap<OwnedRoomId, Vec<Notification>>,
}

impl From<BaseSyncResponse> for SyncResponse {
    fn from(value: BaseSyncResponse) -> Self {
        Self {
            rooms: value.rooms,
            presence: value.presence,
            account_data: value.account_data,
            to_device_events: value.to_device_events,
            device_lists: value.device_lists,
            device_one_time_keys_count: value.device_one_time_keys_count,
            notifications: value.notifications,
        }
    }
}
