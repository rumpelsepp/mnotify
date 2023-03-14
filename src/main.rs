use std::env;
use std::path::PathBuf;

use anyhow::bail;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use futures::StreamExt;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::deserialized_responses::SyncTimelineEvent;
use matrix_sdk::room::Room;
use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use matrix_sdk::ruma::events::receipt::ReceiptThread;
use matrix_sdk::ruma::presence::PresenceState;
use matrix_sdk::ruma::{events::AnySyncTimelineEvent, serde::Raw};
use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};
use serde::Serialize;
use serde_json::value::RawValue;

mod base64;
mod client;
mod mime;
mod outputs;
mod terminal;
mod util;

use crate::client::{session, Client};

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

        /// Query room members
        #[arg(long = "members")]
        query_members: bool,

        /// Query avatars
        #[arg(long = "avatars")]
        query_avatars: bool,
    },
    /// Send a message to a room
    Send {
        /// Enable markdown formatting
        #[arg(short, long)]
        markdown: bool,

        /// Send a notice message
        #[arg(short, long)]
        notice: bool,

        /// Send a emote message
        #[arg(short, long, conflicts_with = "notice")]
        emote: bool,

        #[arg(short, long, required = true)]
        room_id: OwnedRoomId,

        /// Send file as an attachment
        #[arg(short, long)]
        attachment: Option<PathBuf>,

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
    let Room::Joined(room) = room else {return Ok(())};

    let raw_json = event.clone().into_json();
    let parsed_event: SyncTimelineEvent = event.into();
    let event_id = parsed_event.event_id();

    if receipt {
        if let Some(event_id) = event_id {
            room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id)
                .await?;
        }
    }

    println!("{}", raw_json);
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
            let home_server = client.homeserver().await.to_string();
            let user_id = client.user_id().unwrap().to_string();

            #[derive(Serialize)]
            struct HomeserverOutput {
                home_server: String,
                user_id: String,
                token: Option<String>,
            }

            let mut out = HomeserverOutput {
                home_server,
                user_id,
                token: None,
            };

            if include_token {
                if !force {
                    eprintln!("!!!!!!!!!!!!!!!!!!!!!! WARNING !!!!!!!!!!!!!!!!!!!!!!!!!");
                    eprintln!("!!        Keep this token secret at all times         !!");
                    eprintln!("!! Do not publish it and do not store it as plaintext !!");
                    eprintln!("!!!!!!!!!!!!!!!!!!!!!! WARNING !!!!!!!!!!!!!!!!!!!!!!!!!");
                    eprintln!();
                    eprintln!(
                        "Use -f/--force to display the token if you know what you are doing!"
                    );
                    std::process::exit(1);
                }
                out.token = client.access_token();
            }

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
                None => terminal::read_password()?,
                Some(p) => p,
            };

            if let Err(e) = client.login_password(&password).await {
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
        Command::Messages { room_id, limit } => {
            let msgs = client.messages(room_id, limit).await?;
            let events: Vec<Box<RawValue>> = msgs
                .chunk
                .into_iter()
                .map(|e| e.event.into_json())
                .rev()
                .collect();

            println!("{}", serde_json::to_string(&events)?);
        }
        Command::Rooms {
            room_id,
            query_members,
            query_avatars,
        } => {
            let out = match room_id {
                Some(room_id) => {
                    let Some(room) = client.get_room(&room_id) else {
                        bail!("no such room: {}", room_id);
                    };
                    let output = client
                        .query_room(room, query_avatars, query_members)
                        .await?;
                    serde_json::to_string(&output)?
                }
                None => {
                    let mut output = vec![];
                    for room in client.rooms() {
                        output.push(
                            client
                                .query_room(room, query_avatars, query_members)
                                .await?,
                        );
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
            client
                .redact(&room_id, &event_id, reason.as_ref().map(String::as_ref))
                .await?;
        }
        Command::Verify {} => {
            client.set_sas_handlers().await?;
            client.sync(sync_settings.clone()).await?;
        }
        Command::Send {
            markdown,
            notice,
            emote,
            room_id,
            attachment,
            message,
        } => {
            if let Some(path) = attachment {
                client.send_attachment(room_id, path).await?;
            } else {
                let message = match message {
                    Some(message) => message,
                    None => terminal::read_stdin_to_string()?,
                };

                if markdown {
                    if notice {
                        client.send_notice_md(room_id, &message).await?;
                    } else if emote {
                        client.send_emote_md(room_id, &message).await?;
                    } else {
                        client.send_message_md(room_id, &message).await?;
                    }
                } else if notice {
                    client.send_notice(room_id, &message).await?;
                } else if emote {
                    client.send_emote(room_id, &message).await?;
                } else {
                    client.send_message(room_id, &message).await?;
                }
            }
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
                if let Some(ref room_id) = room_id {
                    client.add_room_event_handler(room_id, move |event, room| async move {
                        on_room_message(event, room, receipt).await
                    });
                } else {
                    client.add_event_handler(move |event, room| async move {
                        on_room_message(event, room, receipt).await
                    });
                }

                client.sync(sync_settings.clone()).await?;
            }
        }
        Command::Typing { room_id, disable } => {
            client.send_typing(room_id, !disable).await?;
        }
        Command::Whoami => {
            let resp = client.whoami().await?;
            println!("{}", serde_json::to_string(&resp)?);
        }
    };

    Ok(())
}
