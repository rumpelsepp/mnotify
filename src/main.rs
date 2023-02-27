use std::env;

use anyhow::bail;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::deserialized_responses::SyncTimelineEvent;
use matrix_sdk::room::Room;
use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use matrix_sdk::ruma::events::receipt::ReceiptThread;
use matrix_sdk::ruma::{events::AnySyncTimelineEvent, serde::Raw};
use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};

mod client;
mod config;
mod session;
mod terminal;

use crate::client::Client;
use crate::config::Config;

const CRATE_NAME: &str = clap::crate_name!();

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity,

    #[arg(short, long)]
    full_state: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Login {
        user_id: OwnedUserId,

        #[arg(short, long)]
        password: Option<String>,

        #[arg(short, long, default_value = CRATE_NAME)]
        device_name: String,
    },
    Logout {},
    Redact {
        #[arg(short, long, required = true)]
        room_id: OwnedRoomId,

        #[arg(short, long, required = true)]
        event_id: OwnedEventId,

        #[arg(long)]
        reason: Option<String>,
    },
    Verify {},
    Send {
        #[arg(short, long)]
        markdown: bool,

        #[arg(short, long, required = true)]
        room_id: OwnedRoomId,

        message: Option<String>,
    },
    Sync {
        #[arg(long)]
        room_id: Option<OwnedRoomId>,
    },
    Whoami,
}

async fn on_room_message(event: Raw<AnySyncTimelineEvent>, room: Room) -> anyhow::Result<()> {
    let Room::Joined(room) = room else {return Ok(())};

    let event: SyncTimelineEvent = event.into();
    let event_id = event.event_id();

    if let Some(event_id) = event_id {
        room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id)
            .await?;
    }

    println!("{}", serde_json::to_string(&event)?);
    Ok(())
}

fn convert_filter(filter: log::LevelFilter) -> tracing_subscriber::filter::LevelFilter {
    match filter {
        log::LevelFilter::Off => tracing_subscriber::filter::LevelFilter::OFF,
        log::LevelFilter::Error => tracing_subscriber::filter::LevelFilter::ERROR,
        log::LevelFilter::Warn => tracing_subscriber::filter::LevelFilter::WARN,
        log::LevelFilter::Info => tracing_subscriber::filter::LevelFilter::INFO,
        log::LevelFilter::Debug => tracing_subscriber::filter::LevelFilter::DEBUG,
        log::LevelFilter::Trace => tracing_subscriber::filter::LevelFilter::TRACE,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let sync_settings = SyncSettings::default().full_state(args.full_state);

    tracing_subscriber::fmt()
        .with_max_level(convert_filter(args.verbose.log_level_filter()))
        .init();

    let client = match args.command {
        Command::Login {
            ref user_id,
            ref device_name,
            password: _,
        } => {
            Client::builder()
                .user_id(user_id.to_owned())
                .device_name(device_name.to_owned())
                .build()
                .await?
        }
        _ => Client::builder()
            .load_config()?
            .build()
            .await?
            .ensure_login()?,
    };

    match args.command {
        Command::Login {
            user_id,
            device_name,
            password,
        } => {
            if client.logged_in() {
                bail!("already logged in");
            }

            if Config::exists()? {
                bail!("config exists");
            }

            let password = match password {
                None => terminal::read_password()?,
                Some(p) => p,
            };

            let res = client.login_password(&password).await;
            if let Err(e) = res {
                if terminal::confirm("login failed: delete all state?").await? {
                    // TODO: log this
                    let _ = client.delete_session();
                    let _ = client.delete_state_store();
                }
                bail!("login failed: {}", e);
            }

            // TODO: Do not overwrite stuff.
            Config {
                user_id,
                device_name: Some(device_name),
            }
            .dump()?;
        }
        Command::Logout {} => {
            client.logout().await?;
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
        Command::Sync { room_id } => {
            if let Some(ref room_id) = room_id {
                client.add_room_event_handler(room_id, on_room_message);
            } else {
                client.add_event_handler(on_room_message);
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