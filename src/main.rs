use std::env;

use anyhow::bail;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::deserialized_responses::SyncTimelineEvent;
use matrix_sdk::room::Room;
use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use matrix_sdk::ruma::events::receipt::ReceiptThread;
use matrix_sdk::ruma::presence::PresenceState;
use matrix_sdk::ruma::{events::AnySyncTimelineEvent, serde::Raw};
use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};
use serde_json::value::RawValue;

mod client;
mod session;
mod terminal;
mod util;

use crate::client::Client;

const CRATE_NAME: &str = clap::crate_name!();

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity,

    /// Request the full state during sync
    #[arg(short, long)]
    full_state: bool,

    #[arg(short, long, default_value = "online")]
    presense: PresenceState,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Delete session store and secrets (dangerous!)
    Clean { user_id: OwnedUserId },
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

        /// Dump state events instead
        #[arg(short, long)]
        state: bool,

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
    /// React to emojic verification requests
    Verify {},
    /// Send a message to a room
    Send {
        #[arg(short, long)]
        markdown: bool,

        #[arg(short, long, required = true)]
        room_id: OwnedRoomId,

        message: Option<String>,
    },
    /// Run sync and print all events as json
    Sync {
        #[arg(long)]
        room_id: Option<OwnedRoomId>,

        /// Mark all received messages as read
        #[arg(long)]
        receipt: bool,
    },
    /// Ask the homeserver who we are
    Whoami,
}

async fn on_room_message(
    event: Raw<AnySyncTimelineEvent>,
    room: Room,
    receipt: bool,
) -> anyhow::Result<()> {
    let Room::Joined(room) = room else {return Ok(())};

    let event: SyncTimelineEvent = event.into();
    let event_id = event.event_id();

    if receipt {
        if let Some(event_id) = event_id {
            room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id)
                .await?;
        }
    }

    println!("{}", serde_json::to_string(&event)?);
    Ok(())
}

async fn create_client(cmd: &Command) -> anyhow::Result<Client> {
    match cmd {
        Command::Login {
            ref user_id,
            ref device_name,
            password: _,
        } => {
            Client::builder()
                .user_id(user_id.to_owned())
                .device_name(device_name.to_owned())
                .build()
                .await
        }
        Command::Clean { user_id } => Client::builder().user_id(user_id.to_owned()).build().await,
        _ => Client::builder().load_meta()?.build().await?.ensure_login(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let sync_settings = SyncSettings::default()
        .full_state(args.full_state)
        .set_presence(args.presense);

    tracing_subscriber::fmt()
        .with_max_level(util::convert_filter(args.verbose.log_level_filter()))
        .init();

    let client = create_client(&args.command).await?;

    match args.command {
        Command::Clean { .. } => {
            client.clean()?;
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
                None => terminal::read_password()?,
                Some(p) => p,
            };

            let res = client.login_password(&password).await;
            if let Err(e) = res {
                bail!("login failed: {}", e);
            }

            session::Meta {
                user_id,
                device_name: Some(device_name),
            }
            .dump()?;
        }
        Command::Logout {} => {
            client.logout().await?;
        }
        Command::Messages {
            room_id,
            state,
            limit,
        } => {
            let msgs = client.messages(sync_settings, room_id, limit).await?;
            let events: Vec<Box<RawValue>> = msgs
                .chunk
                .into_iter()
                .map(|e| e.event.into_json())
                .collect();

            println!("{}", serde_json::to_string(&events)?);
        }
        Command::Redact {
            room_id,
            event_id,
            reason,
        } => {
            client
                .redact(&room_id, &event_id, reason.as_ref().map(String::as_ref))
                .await?;
        }
        Command::Verify {} => {
            client.sync_sas_verification().await?;
        }
        Command::Send {
            markdown,
            room_id,
            message,
        } => {
            let message = match message {
                Some(message) => message,
                None => terminal::read_stdin_to_string()?,
            };

            if markdown {
                client
                    .send_message_md(sync_settings, room_id, &message)
                    .await?;
            } else {
                client
                    .send_message(sync_settings, room_id, &message)
                    .await?;
            }
        }
        Command::Sync { room_id, receipt } => {
            if let Some(ref room_id) = room_id {
                client.add_room_event_handler(room_id, move |event, room| async move {
                    on_room_message(event, room, receipt).await
                });
            } else {
                client.add_event_handler(move |event, room| async move {
                    on_room_message(event, room, receipt).await
                });
            }

            client.sync(sync_settings).await?;
        }
        Command::Whoami => {
            let resp = client.whoami().await?;
            println!("{}", serde_json::to_string(&resp)?);
        }
    };

    Ok(())
}
