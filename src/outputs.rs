use matrix_sdk::sync::UnreadNotificationsCount;
use serde::Serialize;

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
