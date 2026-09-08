use std::env;
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use futures::StreamExt;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use matrix_sdk::ruma::events::AnySyncTimelineEvent;
use matrix_sdk::ruma::events::receipt::ReceiptThread;
use matrix_sdk::ruma::presence::PresenceState;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};
use matrix_sdk::{Room, RoomState};
use serde::Serialize;
use serde_json::value::RawValue;

mod client;
mod mime;
mod outputs;
mod terminal;

use crate::client::{Client, TextKind, session};

const CRATE_NAME: &str = clap::crate_name!();

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity,

    /// Request the full state during sync
    #[arg(short, long)]
    full_state: bool,

    /// Presence value while syncing
    #[arg(short, long, default_value = "online")]
    presense: PresenceState,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Delete session store and secrets (dangerous!)
    Clean { user_id: OwnedUserId },
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
    /// Login to a homeserver and create a session store
    Login {
        user_id: OwnedUserId,

        #[arg(short, long)]
        password: Option<String>,

        #[arg(short, long, default_value = CRATE_NAME)]
        device_name: String,
    },
    /// Logout and delete all state
    Logout {},
    /// Dump messages of a room
    Messages {
        #[arg(short, long, required = true)]
        room_id: OwnedRoomId,

        /// Dump all event types
        // #[arg(short, long)]
        // all_types: bool,

        /// Only request this number of events
        #[arg(short, long, default_value = "10")]
        limit: u64,
    },
    /// Redact a specific event
    Redact {
        #[arg(short, long, required = true)]
        room_id: OwnedRoomId,

        #[arg(short, long, required = true)]
        event_id: OwnedEventId,

        #[arg(long)]
        reason: Option<String>,
    },
    /// Query room information
    Rooms {
        /// Only query this room
        #[arg(long)]
        room_id: Option<OwnedRoomId>,
    },
    /// Send a message to a room
    Send {
        #[arg(short, long, required = true)]
        room_id: OwnedRoomId,

        /// Enable markdown formatting
        #[arg(short, long)]
        markdown: bool,

        /// Send a notice message
        #[arg(short, long)]
        notice: bool,

        /// Send an emote message
        #[arg(short, long, conflicts_with = "notice")]
        emote: bool,

        /// Send file as an attachment
        #[arg(short, long, conflicts_with = "message")]
        attachment: Option<PathBuf>,

        /// Reply to a specific event_id
        #[arg(long, conflicts_with_all = ["notice", "emote", "attachment"])]
        reply_to: Option<OwnedEventId>,

        /// String to send; read from stdin if omitted
        message: Option<String>,
    },
    /// Run sync and print all events
    Sync {
        #[arg(long)]
        room_id: Option<OwnedRoomId>,

        /// Mark all received messages as read
        #[arg(long)]
        receipt: bool,

        /// Print raw sync events as they come
        #[arg(long)]
        raw: bool,
    },
    /// Send typing notifications
    Typing {
        #[arg(long, required = true)]
        room_id: OwnedRoomId,

        /// Disable typing
        #[arg(long)]
        disable: bool,
    },
    /// React to emojic verification requests
    Verify {},
    /// Ask the homeserver who we are
    Whoami,
}

impl Command {
    fn can_sync(&self) -> bool {
        !matches!(
            self,
            Command::Clean { .. } | Command::Login { .. } | Command::Sync { .. }
        )
    }
}

async fn on_room_message(
    event: Raw<AnySyncTimelineEvent>,
    room: Room,
    receipt: bool,
) -> anyhow::Result<()> {
    if room.state() != RoomState::Joined {
        return Ok(());
    }

    if receipt && let Some(event_id) = event.get_field::<OwnedEventId>("event_id")? {
        room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id)
            .await?;
    }

    println!("{}", event.into_json());
    Ok(())
}

async fn create_client(cmd: &Command) -> anyhow::Result<Client> {
    match cmd {
        Command::Login {
            user_id,
            device_name,
            ..
        } => Client::new(user_id.clone(), device_name.clone()).await,
        Command::Clean { user_id } => Client::new(user_id.clone(), CRATE_NAME.to_string()).await,
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
        .full_state(args.full_state)
        .set_presence(args.presense);

    // Logs go to stderr so they never corrupt the JSON on stdout. `RUST_LOG`
    // wins if set, otherwise the verbosity flags decide the level.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(args.verbose.tracing_level_filter().to_string())
    });
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();

    let client = create_client(&args.command).await?;

    if args.command.can_sync() {
        client.sync_once(sync_settings.clone()).await?;
    }

    match args.command {
        Command::Clean { .. } => {
            client.clean()?;
        }
        Command::Homeserver {
            force,
            include_token,
        } => {
            let token = if include_token {
                if !force {
                    eprintln!(
                        "Refusing to print the access token without -f/--force.\n\
                         Keep it secret: it grants full access to your account and must \
                         never be published or stored as plaintext."
                    );
                    std::process::exit(1);
                }
                client.access_token()
            } else {
                None
            };

            #[derive(Serialize)]
            struct HomeserverOutput {
                home_server: String,
                user_id: String,
                token: Option<String>,
            }

            let out = HomeserverOutput {
                home_server: client.homeserver().to_string(),
                user_id: client.user_id().unwrap().to_string(),
                token,
            };

            println!("{}", serde_json::to_string(&out)?);
        }
        Command::Login {
            user_id,
            device_name,
            password,
        } => {
            if client.logged_in() {
                bail!("already logged in");
            }

            if session::Meta::exists()? {
                bail!("meta exists");
            }

            let password = match password {
                Some(p) => p,
                None => terminal::read_password()?,
            };

            client
                .login_password(&password)
                .await
                .context("login failed")?;

            session::Meta {
                user_id,
                device_name: Some(device_name),
            }
            .dump()?;
        }
        Command::Logout {} => {
            client.logout().await?;
        }
        Command::Messages { room_id, limit } => {
            let msgs = client.messages(room_id, limit).await?;
            let events: Vec<Box<RawValue>> = msgs
                .chunk
                .into_iter()
                .map(|e| e.into_raw().into_json())
                .rev()
                .collect();

            println!("{}", serde_json::to_string(&events)?);
        }
        Command::Rooms { room_id } => {
            let out = match room_id {
                Some(room_id) => {
                    let Some(room) = client.get_room(&room_id) else {
                        bail!("no such room: {}", room_id);
                    };
                    let output = client.query_room(room).await?;
                    serde_json::to_string(&output)?
                }
                None => {
                    let mut output = Vec::new();
                    for room in client.rooms() {
                        output.push(client.query_room(room).await?);
                    }
                    serde_json::to_string(&output)?
                }
            };

            println!("{}", out);
        }
        Command::Redact {
            room_id,
            event_id,
            reason,
        } => {
            let room = client.get_joined_room(room_id)?;
            room.redact(&event_id, reason.as_deref(), None).await?;
        }
        Command::Verify {} => {
            client.set_sas_handlers().await?;
            client.sync(sync_settings.clone()).await?;
        }
        Command::Send {
            room_id,
            reply_to,
            markdown,
            notice,
            emote,
            attachment,
            message,
        } => {
            if let Some(path) = attachment {
                return client.send_attachment(room_id, path).await;
            }

            let body = match message {
                Some(message) => message,
                None => terminal::read_stdin_to_string()?,
            };

            if let Some(event_id) = &reply_to {
                return client
                    .send_message_reply(room_id, event_id, &body, markdown)
                    .await;
            }

            let kind = if notice {
                TextKind::Notice
            } else if emote {
                TextKind::Emote
            } else {
                TextKind::Text
            };
            client.send_text(room_id, &body, kind, markdown).await?;
        }
        Command::Sync {
            room_id,
            receipt,
            raw,
        } => {
            if raw {
                let mut sync_stream = Box::pin(client.sync_stream(sync_settings.clone()).await);
                while let Some(Ok(response)) = sync_stream.next().await {
                    let resp: outputs::SyncResponse = response.into();
                    println!("{}", serde_json::to_string(&resp)?);
                }
            } else {
                match &room_id {
                    Some(room_id) => {
                        client.add_room_event_handler(room_id, move |event, room| async move {
                            on_room_message(event, room, receipt).await
                        });
                    }
                    None => {
                        client.add_event_handler(move |event, room| async move {
                            on_room_message(event, room, receipt).await
                        });
                    }
                }

                client.sync(sync_settings.clone()).await?;
            }
        }
        Command::Typing { room_id, disable } => {
            let room = client.get_joined_room(room_id)?;
            room.typing_notice(!disable).await?;
        }
        Command::Whoami => {
            let resp = client.whoami().await?;
            let out = serde_json::json!({
                "user_id": resp.user_id,
                "device_id": resp.device_id,
                "is_guest": resp.is_guest,
            });
            println!("{out}");
        }
    };

    Ok(())
}
