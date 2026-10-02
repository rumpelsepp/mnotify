use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::deserialized_responses::EncryptionInfo;
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use matrix_sdk::ruma::events::receipt::ReceiptThread;
use matrix_sdk::ruma::events::{AnySyncTimelineEvent, Mentions};
use matrix_sdk::ruma::presence::PresenceState;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedEventId, OwnedRoomOrAliasId, OwnedUserId};
use matrix_sdk::{Room, RoomState};

mod client;
mod output;
mod terminal;

use crate::client::recovery::NOT_CROSS_SIGNED;
use crate::client::sync::Scope;
use crate::client::{Addressing, Client, Relation, TextKind, session};
use crate::output::{Events, Record, Rooms};

const CRATE_NAME: &str = clap::crate_name!();

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity,

    /// Print machine-readable JSON instead of tables and text
    #[arg(long, global = true)]
    json: bool,

    /// Request the full state during sync
    #[arg(long)]
    full_state: bool,

    /// Presence to announce while syncing
    #[arg(long, default_value = "online")]
    presence: PresenceState,

    #[command(subcommand)]
    command: Command,
}

/// The room a command acts on.
#[derive(Args, Debug)]
struct RoomArg {
    /// Room ID (!abc:example.org) or alias (#ops:example.org)
    #[arg(short, long, env = "MN_ROOM")]
    room: OwnedRoomOrAliasId,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Delete the local session, store and secrets of a user (no server call)
    Clean {
        /// Full Matrix ID, e.g. @bot:example.org
        user_id: OwnedUserId,
    },
    /// Get information about your homeserver and login
    #[command(alias = "hs")]
    Homeserver {
        /// Really print the token
        #[arg(short, long)]
        force: bool,

        /// Include the bearer token
        #[arg(short = 't', long = "token")]
        include_token: bool,
    },
    /// Join a room or accept an invite
    Join {
        /// Room ID (!abc:example.org) or alias (#ops:example.org)
        room: OwnedRoomOrAliasId,
    },
    /// Log in and create the local session store
    Login {
        /// Full Matrix ID, e.g. @bot:example.org
        user_id: OwnedUserId,

        /// Log in via the OAuth 2.0 device grant: `mn` shows a QR code to scan
        #[arg(long, conflicts_with = "sso")]
        qr: bool,

        /// Log in via the homeserver's SSO flow (e.g. SAML) in a browser
        #[arg(long)]
        sso: bool,

        /// SSO identity provider id (only with --sso; omit to use the server's picker)
        #[arg(long, requires = "sso")]
        idp: Option<String>,

        /// Device name shown to other clients
        #[arg(short, long, default_value = CRATE_NAME)]
        device_name: String,

        /// Homeserver URL; only needed if the server has no .well-known for it
        #[arg(long, value_name = "URL")]
        homeserver: Option<String>,
    },
    /// Log out on the server and delete all local state
    Logout,
    /// Dump the latest messages of a room
    Messages {
        #[command(flatten)]
        room: RoomArg,

        /// Number of events to fetch
        #[arg(short, long, default_value = "10")]
        limit: u64,

        /// Only this thread: the event that started it, then the latest replies
        #[arg(long, value_name = "EVENT_ID")]
        thread: Option<OwnedEventId>,
    },
    /// Redact (delete) an event
    Redact {
        #[command(flatten)]
        room: RoomArg,

        /// ID of the event to redact
        #[arg(short, long)]
        event_id: OwnedEventId,

        /// Reason shown to other members
        #[arg(long)]
        reason: Option<String>,
    },
    /// Query room information
    Rooms {
        /// Only query this room (ID or alias)
        #[arg(short, long)]
        room: Option<OwnedRoomOrAliasId>,
    },
    /// Send a message or file to a room; prints the event ID
    Send {
        #[command(flatten)]
        room: RoomArg,

        /// Render the message as Markdown
        #[arg(short, long, conflicts_with = "attachment")]
        markdown: bool,

        /// Send as notice (m.notice, the convention for bots)
        #[arg(short, long)]
        notice: bool,

        /// Send as emote (/me)
        #[arg(short, long, conflicts_with = "notice")]
        emote: bool,

        /// Send this file instead of a text message
        #[arg(short, long, conflicts_with_all = ["message", "notice", "emote"])]
        attachment: Option<PathBuf>,

        /// Reply to this event; stays in its thread if it is in one
        #[arg(long, value_name = "EVENT_ID")]
        reply_to: Option<OwnedEventId>,

        /// Post into the thread of this event, starting one if there is none
        #[arg(long, value_name = "EVENT_ID", conflicts_with = "reply_to")]
        thread: Option<OwnedEventId>,

        /// Notify this user (repeatable)
        #[arg(long, value_name = "USER_ID")]
        mention: Vec<OwnedUserId>,

        /// Notify everyone in the room (@room); needs the power level for it
        #[arg(long)]
        mention_room: bool,

        /// Message text; read from stdin if omitted
        message: Option<String>,
    },
    /// Sync forever and print incoming timeline events as JSON lines
    Sync {
        /// Only print events of this room (ID or alias)
        #[arg(short, long)]
        room: Option<OwnedRoomOrAliasId>,

        /// Mark all received messages as read
        #[arg(long)]
        receipt: bool,
    },
    /// Send typing notifications
    Typing {
        #[command(flatten)]
        room: RoomArg,

        /// Disable typing
        #[arg(long)]
        disable: bool,
    },
    /// Verify this device with another of your devices: wait for a request started there, or start one with --device
    Verify {
        /// Device ID of one of your own devices to start verifying
        #[arg(long)]
        device: Option<matrix_sdk::ruma::OwnedDeviceId>,
    },
    /// Manage key backup and cross-signing recovery
    Recovery {
        #[command(subcommand)]
        action: RecoveryAction,
    },
    /// Ask the homeserver who we are
    Whoami,
}

#[derive(Debug, Subcommand)]
enum RecoveryAction {
    /// Print recovery, key-backup and cross-signing status
    Status,
    /// Bootstrap cross-signing and key backup, then print the recovery key
    Enable,
    /// Restore secrets from a recovery key (read from stdin if omitted)
    Recover { recovery_key: Option<String> },
    /// Replace the recovery key with a fresh one
    Reset,
    /// Turn off recovery and delete the server-side key backup
    Disable,
}

/// What a command needs synced before it runs.
enum SyncNeed<'a> {
    Nothing,
    Account,
    Room(&'a OwnedRoomOrAliasId),
    AllRooms,
}

impl Command {
    fn sync_need(&self) -> SyncNeed<'_> {
        match self {
            // `sync` runs its own loop; `login` and `clean` have no session yet.
            Command::Clean { .. } | Command::Login { .. } | Command::Sync { .. } => {
                SyncNeed::Nothing
            }
            Command::Messages { room, .. }
            | Command::Redact { room, .. }
            | Command::Send { room, .. }
            | Command::Typing { room, .. } => SyncNeed::Room(&room.room),
            Command::Rooms { room: Some(room) } => SyncNeed::Room(room),
            Command::Rooms { room: None } => SyncNeed::AllRooms,
            Command::Homeserver { .. }
            | Command::Join { .. }
            | Command::Logout
            | Command::Verify { .. }
            | Command::Recovery { .. }
            | Command::Whoami => SyncNeed::Account,
        }
    }
}

async fn on_room_message(
    event: Raw<AnySyncTimelineEvent>,
    room: Room,
    encryption_info: Option<EncryptionInfo>,
    receipt: bool,
    json: bool,
) -> anyhow::Result<()> {
    if room.state() != RoomState::Joined {
        return Ok(());
    }

    if receipt && let Some(event_id) = event.get_field::<OwnedEventId>("event_id")? {
        room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id)
            .await?;
    }

    if json {
        println!("{}", event.json());
    } else {
        let unproven = output::sender_unproven(encryption_info.as_ref());
        println!("{}", output::event_line(event.json(), unproven));
    }
    Ok(())
}

async fn create_client(cmd: &Command) -> anyhow::Result<Client> {
    match cmd {
        Command::Login {
            user_id,
            device_name,
            homeserver,
            ..
        } => {
            if session::Meta::exists()? {
                let current = session::Meta::load()
                    .map(|m| m.user_id.to_string())
                    .unwrap_or_else(|_| "another user".into());
                bail!("already logged in as {current}; run `mn logout` first");
            }
            Client::new(user_id.clone(), device_name.clone(), homeserver.as_deref()).await
        }
        _ => Client::from_meta().await?.ensure_logged_in(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    // Lazy-load room members: a large account syncs a lot faster this way, and
    // the sync token is persisted in the SQLite store by the SDK, so repeated
    // invocations only fetch the delta.
    let sync_settings = SyncSettings::default()
        .filter(FilterDefinition::with_lazy_loading().into())
        .full_state(args.full_state);

    // Logs go to stderr so they never corrupt the JSON on stdout. `RUST_LOG`
    // wins if set, otherwise the verbosity flags decide the level.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let mut directives = args.verbose.tracing_level_filter().to_string();
        // At the default level the SDK logs expected 404s (e.g. "Account
        // data not found") as errors; real failures still reach us as `Err`.
        if !args.verbose.is_present() {
            directives.push_str(",matrix_sdk::http_client=off");
        }
        tracing_subscriber::EnvFilter::new(directives)
    });
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    if let Command::Clean { user_id } = &args.command {
        session::clean(user_id);
        return Ok(());
    }

    let client = create_client(&args.command).await?;
    // The default for every sync request, /v3/sync and sliding sync alike.
    client.set_presence(args.presence, None, false).await?;

    let result = run(args.command, &client, sync_settings, args.json).await;

    // Let background E2EE setup finish (also after an error): exiting while it
    // runs makes the SDK log spurious errors or even panic on shutdown.
    client
        .encryption()
        .wait_for_e2ee_initialization_tasks()
        .await;
    result
}

async fn run(
    command: Command,
    client: &Client,
    sync_settings: SyncSettings,
    json: bool,
) -> anyhow::Result<()> {
    match command.sync_need() {
        SyncNeed::Nothing => {}
        SyncNeed::Account => {
            client
                .catch_up(Scope::Account, sync_settings.clone())
                .await?
        }
        SyncNeed::AllRooms => {
            client
                .catch_up(Scope::AllRooms, sync_settings.clone())
                .await?
        }
        SyncNeed::Room(room) => {
            let room_id = client.resolve_room_id(room).await?;
            client
                .catch_up(Scope::Room(&room_id), sync_settings.clone())
                .await?
        }
    }

    match command {
        Command::Clean { .. } => unreachable!("handled before the client is built"),
        Command::Homeserver {
            force,
            include_token,
        } => {
            let token = if include_token {
                if !force {
                    bail!(
                        "refusing to print the access token without -f/--force; \
                         it grants full access to your account"
                    );
                }
                client.access_token()
            } else {
                None
            };

            let out = Record::new()
                .field("home_server", client.homeserver().as_str())
                .field("user_id", client.user_id().context("not logged in")?)
                .field("token", token);
            output::print(json, &out)?;
        }
        Command::Login {
            user_id,
            device_name,
            qr,
            sso,
            idp,
            ..
        } => {
            if client.logged_in() {
                bail!("{user_id} is already logged in; run `mn logout` first");
            }

            if sso {
                client
                    .login_sso(idp.as_deref())
                    .await
                    .context("SSO login failed")?;
            } else if qr {
                client.login_qr().await.context("QR login failed")?;
            } else {
                let password = terminal::read_password()?;
                client
                    .login_password(&password)
                    .await
                    .context("login failed")?;
            }

            anyhow::ensure!(
                client.user_id().is_some_and(|u| u == user_id),
                "logged in as {:?}, not {user_id}",
                client.user_id(),
            );

            if let Some(path) = session::secrets_file(&user_id)? {
                eprintln!(
                    "warning: no system keyring in use; the access token and the store \
                     passphrase are kept in plain text in {} (mode 0600). Anyone who can \
                     read this file can act as {user_id}.",
                    path.display()
                );
            }

            // The SDK creates cross-signing keys in the background if the
            // account has none yet; wait for it before reporting the state.
            client
                .encryption()
                .wait_for_e2ee_initialization_tasks()
                .await;
            if !client.is_cross_signed().await? {
                eprintln!("warning: {NOT_CROSS_SIGNED}.");
            } else if !client.recovery_enabled() {
                eprintln!(
                    "hint: run `mn recovery enable` to keep the cross-signing keys in \
                     secret storage; without it, a future login cannot be cross-signed."
                );
            }

            session::Meta {
                user_id,
                device_name,
                homeserver: client.homeserver().to_string(),
                sliding_sync: client.detect_sliding_sync().await,
            }
            .dump()?;
        }
        Command::Join { room } => {
            let joined = client.join(&room).await?;
            let out = Record::new()
                .field("room_id", joined.room_id())
                .headline("room_id");
            output::print(json, &out)?;
        }
        Command::Logout => {
            client.logout().await?;
        }
        Command::Messages {
            room,
            limit,
            thread,
        } => {
            let room = client.joined_room(&room.room).await?;
            let events = match thread {
                Some(root) => client.thread(&room, &root, limit).await?,
                None => client.messages(&room, limit).await?,
            };
            let events = events.into_iter().map(|e| output::Event {
                unproven: output::sender_unproven(e.encryption_info().map(|i| &**i)),
                raw: e.into_raw().into_json(),
            });
            output::print(json, &Events(events.collect()))?;
        }
        Command::Rooms { room } => match room {
            Some(room) => {
                let room_id = client.resolve_room_id(&room).await?;
                let Some(room) = client.get_room(&room_id) else {
                    bail!("unknown room: {room}");
                };
                output::print(json, &client.query_room(room).await?)?;
            }
            None => {
                let mut rooms = Vec::new();
                for room in client.rooms() {
                    rooms.push(client.query_room(room).await?);
                }
                rooms.sort_by(|a, b| a.display_name.cmp(&b.display_name));
                output::print(json, &Rooms(rooms))?;
            }
        },
        Command::Redact {
            room,
            event_id,
            reason,
        } => {
            let room = client.joined_room(&room.room).await?;
            room.redact(&event_id, reason.as_deref(), None).await?;
        }
        Command::Verify { device } => match device {
            Some(device_id) => {
                tokio::select! {
                    r = client.verify_device(&device_id) => r?,
                    r = client.sync_forever(Scope::Account, sync_settings.clone()) => r?,
                }
            }
            None => {
                client.set_sas_handlers().await?;
                let device_id = client.device_id().map(|d| d.as_str()).unwrap_or("?");
                eprintln!(
                    "Waiting for a verification request. Start it on the other device \
                     by selecting this session ({device_id}) in its session list."
                );
                client
                    .sync_forever(Scope::Account, sync_settings.clone())
                    .await?;
            }
        },
        Command::Recovery { action } => match action {
            RecoveryAction::Status => {
                output::print(json, &client.recovery_status().await?)?;
            }
            RecoveryAction::Enable => {
                let key = client.recovery_enable().await?;
                let out = Record::new()
                    .field("recovery_key", key)
                    .headline("recovery_key");
                output::print(json, &out)?;
            }
            RecoveryAction::Recover { recovery_key } => {
                let key = match recovery_key {
                    Some(k) => k,
                    None => terminal::read_stdin_to_string()?,
                };
                client.recovery_recover(&key).await?;
            }
            RecoveryAction::Reset => {
                let key = client.recovery_reset().await?;
                let out = Record::new()
                    .field("recovery_key", key)
                    .headline("recovery_key");
                output::print(json, &out)?;
            }
            RecoveryAction::Disable => {
                client.recovery_disable().await?;
            }
        },
        Command::Send {
            room,
            reply_to,
            thread,
            mention,
            mention_room,
            markdown,
            notice,
            emote,
            attachment,
            message,
        } => {
            let room = client.joined_room(&room.room).await?;
            anyhow::ensure!(
                !room.latest_encryption_state().await?.is_encrypted()
                    || client.is_cross_signed().await?,
                "{NOT_CROSS_SIGNED}"
            );

            if mention_room {
                client.ensure_can_mention_room(&room).await?;
            }
            let mut mentions = Mentions::with_user_ids(mention);
            mentions.room = mention_room;
            let addressing = Addressing {
                relation: reply_to
                    .map(Relation::Reply)
                    .or(thread.map(Relation::Thread)),
                mentions,
            };
            let event_id = if let Some(path) = attachment {
                client.send_attachment(&room, path, addressing).await?
            } else {
                let body = match message {
                    Some(message) => message,
                    None => terminal::read_message()?,
                };
                anyhow::ensure!(!body.trim().is_empty(), "refusing to send an empty message");

                let kind = if notice {
                    TextKind::Notice
                } else if emote {
                    TextKind::Emote
                } else {
                    TextKind::Text
                };
                client
                    .send_text(&room, &body, kind, markdown, addressing)
                    .await?
            };

            let out = Record::new()
                .field("room_id", room.room_id())
                .field("event_id", event_id)
                .headline("event_id");
            output::print(json, &out)?;
        }
        Command::Sync { room, receipt } => {
            let room_id = match &room {
                Some(room) => Some(client.resolve_room_id(room).await?),
                None => None,
            };
            match &room_id {
                Some(room_id) => {
                    client.add_room_event_handler(room_id, move |event, room, info| async move {
                        on_room_message(event, room, info, receipt, json).await
                    });
                }
                None => {
                    client.add_event_handler(move |event, room, info| async move {
                        on_room_message(event, room, info, receipt, json).await
                    });
                }
            }

            let scope = match &room_id {
                Some(room_id) => Scope::Room(room_id),
                None => Scope::AllRooms,
            };
            client.sync_forever(scope, sync_settings.clone()).await?;
        }
        Command::Typing { room, disable } => {
            let room = client.joined_room(&room.room).await?;
            room.typing_notice(!disable).await?;
        }
        Command::Whoami => {
            let resp = client.whoami().await?;
            let out = Record::new()
                .field("user_id", resp.user_id)
                .field("device_id", resp.device_id)
                .field("is_guest", resp.is_guest);
            output::print(json, &out)?;
        }
    };

    Ok(())
}
