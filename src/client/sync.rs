//! Bringing the local state up to date, via Simplified Sliding Sync (MSC4186)
//! where the homeserver offers it and the classic `/v3/sync` otherwise.
//!
//! A one-shot command (`send`, `messages`, ...) only needs the state of the
//! room it acts on plus the E2EE bits. With sliding sync that is a single
//! small room subscription instead of a `/v3/sync` over every room of the
//! account. The to-device `since` token is persisted by the SDK, so to-device
//! messages are acknowledged across invocations.
//!
//! Which API a login uses is decided once, at login (see `Meta::sliding_sync`).

use std::env;
use std::time::Duration;

use futures::StreamExt;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::ruma::api::client::sync::sync_events::v5 as http;
use matrix_sdk::ruma::events::StateEventType;
use matrix_sdk::ruma::{RoomId, assign};
use matrix_sdk::sliding_sync::{
    SlidingSync, SlidingSyncBuilder, SlidingSyncList, SlidingSyncListLoadingState, SlidingSyncMode,
    Version,
};
use tracing::debug;

/// Which rooms a sync has to cover.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Scope<'a> {
    /// No room state, only E2EE and account data (`whoami`, `recovery`, ...).
    Account,
    /// One room (`send`, `messages`, ...).
    Room(&'a RoomId),
    /// Every room of the account (`rooms`, `sync`).
    AllRooms,
}

const ALL_ROOMS: &str = "all_rooms";

/// Number of rooms fetched per request while growing the room list.
const BATCH_SIZE: u32 = 100;

/// Events per room delivered to a continuous `mn sync`. Sliding sync also
/// caps incremental updates by this, so it must cover bursts.
const LIVE_TIMELINE_LIMIT: u32 = 50;

/// The state a command needs to know about a room: whether it is encrypted,
/// our membership, and what `mn rooms` prints.
fn required_state() -> Vec<(StateEventType, String)> {
    [
        StateEventType::RoomCreate,
        StateEventType::RoomEncryption,
        StateEventType::RoomName,
        StateEventType::RoomTopic,
        StateEventType::RoomAvatar,
        StateEventType::RoomCanonicalAlias,
        StateEventType::RoomJoinRules,
        StateEventType::RoomHistoryVisibility,
        StateEventType::RoomGuestAccess,
        StateEventType::RoomTombstone,
        StateEventType::RoomPowerLevels,
    ]
    .into_iter()
    .map(|t| (t, String::new()))
    .chain([(StateEventType::RoomMember, "$ME".to_owned())])
    .collect()
}

fn all_rooms_list(timeline_limit: u32) -> matrix_sdk::sliding_sync::SlidingSyncListBuilder {
    SlidingSyncList::builder(ALL_ROOMS)
        .sync_mode(SlidingSyncMode::new_growing(BATCH_SIZE))
        .required_state(required_state())
        .timeline_limit(timeline_limit)
}

fn room_subscription(timeline_limit: u32) -> http::request::RoomSubscription {
    assign!(http::request::RoomSubscription::default(), {
        required_state: required_state(),
        timeline_limit: timeline_limit.into(),
    })
}

impl super::Client {
    /// Whether a new login should use sliding sync: the server announces
    /// MSC4186 and it is not switched off with `MN_SLIDING_SYNC=0`.
    pub(crate) async fn detect_sliding_sync(&self) -> bool {
        if env::var_os("MN_SLIDING_SYNC").is_some_and(|v| v == "0") {
            return false;
        }
        let native = self
            .inner
            .available_sliding_sync_versions()
            .await
            .iter()
            .any(|v| matches!(v, Version::Native));
        debug!(native, "sliding sync support");
        native
    }

    /// A sliding sync with the extensions every command needs: E2EE (device
    /// lists, one-time keys), to-device messages and account data (secret
    /// storage, direct rooms).
    fn sliding_sync_builder(&self, id: &str) -> anyhow::Result<SlidingSyncBuilder> {
        Ok(self
            .inner
            .sliding_sync(id)?
            .version(Version::Native)
            .with_e2ee_extension(assign!(http::request::E2EE::default(), {
                enabled: Some(true),
            }))
            .with_to_device_extension(assign!(http::request::ToDevice::default(), {
                enabled: Some(true),
            }))
            .with_account_data_extension(assign!(http::request::AccountData::default(), {
                enabled: Some(true),
            })))
    }

    /// Bring the local state up to date before a one-shot command, covering
    /// only what `scope` needs.
    pub(crate) async fn catch_up(
        &self,
        scope: Scope<'_>,
        settings: SyncSettings,
    ) -> anyhow::Result<()> {
        if !self.sliding_sync {
            self.inner.sync_once(settings).await?;
            return Ok(());
        }

        // A fresh connection per invocation, so concurrent processes never
        // fight over a shared `pos`; the SDK persists the to-device token.
        let mut builder = self
            .sliding_sync_builder("mn-once")?
            .poll_timeout(Duration::ZERO);
        if let Scope::AllRooms = scope {
            builder = builder.add_list(all_rooms_list(0));
        }
        let sliding_sync = builder.build().await?;
        if let Scope::Room(room_id) = scope {
            sliding_sync.add_room_subscriptions(&[room_id], Some(room_subscription(0)), false);
        }

        let stream = sliding_sync.sync();
        futures::pin_mut!(stream);
        while let Some(summary) = stream.next().await {
            summary?;
            if !matches!(scope, Scope::AllRooms) || all_rooms_loaded(&sliding_sync).await {
                break;
            }
        }
        Ok(())
    }

    /// Sync until an error occurs, for `mn sync` and `mn verify`. Event
    /// handlers registered on the client see every event within `scope`.
    pub(crate) async fn sync_forever(
        &self,
        scope: Scope<'_>,
        settings: SyncSettings,
    ) -> anyhow::Result<()> {
        if !self.sliding_sync {
            self.inner.sync(settings).await?;
            return Ok(());
        }

        // Unlike the one-shot catch-up, keep `pos` across restarts so a
        // restarted `mn sync` continues where it stopped.
        let mut builder = self.sliding_sync_builder("mn-live")?.share_pos();
        if let Scope::AllRooms = scope {
            builder = builder.add_list(all_rooms_list(LIVE_TIMELINE_LIMIT));
        }
        let sliding_sync = builder.build().await?;
        if let Scope::Room(room_id) = scope {
            sliding_sync.add_room_subscriptions(
                &[room_id],
                Some(room_subscription(LIVE_TIMELINE_LIMIT)),
                false,
            );
        }

        let stream = sliding_sync.sync();
        futures::pin_mut!(stream);
        while let Some(summary) = stream.next().await {
            summary?;
        }
        Ok(())
    }
}

async fn all_rooms_loaded(sliding_sync: &SlidingSync) -> bool {
    sliding_sync
        .on_list(ALL_ROOMS, |list| {
            std::future::ready(matches!(
                list.state(),
                SlidingSyncListLoadingState::FullyLoaded
            ))
        })
        .await
        .unwrap_or(true)
}
