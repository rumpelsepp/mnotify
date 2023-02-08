use std::fs;
use std::future::Future;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::deserialized_responses::SyncResponse;
use matrix_sdk::event_handler::{EventHandler, EventHandlerHandle, EventHandlerResult, SyncEvent};
use matrix_sdk::room::Room;
use matrix_sdk::ruma::events::room::message::{
    MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
};
use matrix_sdk::ruma::{OwnedRoomId, OwnedUserId, RoomId, UserId};
use matrix_sdk::{Client, LoopCtrl, Session};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const CRATE_NAME: &'static str = env!("CARGO_CRATE_NAME");

fn config_path() -> io::Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(CRATE_NAME)?;
    xdg_dirs.place_config_file("config.toml")
}

#[derive(Serialize, Deserialize)]
struct Config {
    user_id: OwnedUserId,
}

impl Config {
    fn load() -> anyhow::Result<Self> {
        let raw = fs::read_to_string(config_path()?)?;
        Ok(toml::from_str(&raw)?)
    }

    fn dump(&self) -> anyhow::Result<()> {
        let raw = toml::to_string(&self)?;
        fs::write(config_path()?, &raw)?;
        Ok(())
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity,

    #[command(subcommand)]
    command: Command,
}

/// Doc comment
#[derive(Debug, Subcommand)]
enum Command {
    Login {
        user_id: OwnedUserId,
        #[arg(short, long)]
        password: Option<String>,
        #[arg(short, long)]
        device_name: Option<String>,
    },
    Message {
        room_id: OwnedRoomId,
        message: String,
    },
    Sync {
        room_id: Option<OwnedRoomId>,
        #[arg(short, long)]
        raw: bool,
    },
}

fn read_password_stdin() -> io::Result<String> {
    let mut res = String::new();
    // TODO: Remove atty crate:
    // https://doc.rust-lang.org/std/io/struct.Stdin.html#impl-IsTerminal-for-Stdin
    if atty::is(atty::Stream::Stdin) {
        res = rpassword::prompt_password("password: ")?;
    } else {
        io::stdin().read_line(&mut res)?;
    }
    Ok(res)
}

fn load_session(path: impl AsRef<Path>) -> Option<Session> {
    let raw = fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&raw).ok()?)
}

fn persist_session(path: impl AsRef<Path>, session: Session) -> io::Result<()> {
    let out = serde_json::to_string(&session)?;
    fs::write(path, out)?;
    Ok(())
}

struct MnClient {
    inner: Client,
    session_path: PathBuf,
    user_id: OwnedUserId,
    // device_id: OwnedDeviceId,
}

impl MnClient {
    async fn new(user_id: impl AsRef<UserId>) -> anyhow::Result<Self> {
        let user_id = user_id.as_ref();
        let xdg_dirs = xdg::BaseDirectories::with_prefix(CRATE_NAME)?;
        let session_path = xdg_dirs.place_state_file(format!("session_{}.json", user_id))?;
        let state_path = xdg_dirs.place_state_file(format!("state_{}.sled", user_id))?;

        let inner = Client::builder()
            .server_name(user_id.server_name())
            .sled_store(state_path, None)?
            .build()
            .await?;

        if let Some(session) = load_session(&session_path) {
            inner.restore_login(session).await?
        }

        Ok(Self {
            inner,
            session_path,
            user_id: user_id.to_owned(),
        })
    }

    fn logged_in(&self) -> bool {
        self.inner.logged_in()
    }

    async fn login_password(&self, device_name: &str, password: &str) -> anyhow::Result<()> {
        self.inner
            .login_username(&self.user_id, password)
            .initial_device_display_name(device_name)
            .send()
            .await?;

        self.persist_session()?;
        Ok(())
    }

    async fn sync_once(&self) -> anyhow::Result<()> {
        self.inner.sync_once(SyncSettings::default()).await?;
        Ok(())
    }

    pub fn add_event_handler<Ev, Ctx, H>(&self, handler: H) -> EventHandlerHandle
    where
        Ev: SyncEvent + DeserializeOwned + Send + 'static,
        H: EventHandler<Ev, Ctx>,
        <H::Future as Future>::Output: EventHandlerResult,
    {
        self.inner.add_event_handler(handler)
    }

    async fn sync(&self) -> anyhow::Result<()> {
        self.inner.sync(SyncSettings::default()).await?;
        Ok(())
    }

    async fn sync_with_callback<C>(
        &self,
        callback: impl Fn(SyncResponse) -> C,
    ) -> anyhow::Result<()>
    where
        C: Future<Output = LoopCtrl>,
    {
        self.inner
            .sync_with_callback(SyncSettings::default(), callback)
            .await?;
        Ok(())
    }

    async fn send_message(&self, room: impl AsRef<RoomId>, msg: &str) -> anyhow::Result<()> {
        let room = self.inner.get_joined_room(room.as_ref()).unwrap();
        let event = RoomMessageEventContent::text_plain(msg);
        room.send(event, None).await?;
        Ok(())
    }

    // TODO: Use keyring for this!
    fn persist_session(&self) -> io::Result<()> {
        persist_session(&self.session_path, self.inner.session().unwrap())
    }

    // TODO: Use keyring for this!
    fn load_session(&self) -> Option<Session> {
        load_session(&self.session_path)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    match args.command {
        Command::Login {
            user_id,
            password,
            device_name,
        } => {
            let client = MnClient::new(&user_id).await?;
            if !client.logged_in() {
                let password = match password {
                    None => read_password_stdin()?,
                    Some(p) if p == "-" => read_password_stdin()?,
                    Some(p) => p,
                };
                client
                    .login_password(&device_name.unwrap_or(CRATE_NAME.to_string()), &password)
                    .await?;
            }

            Config { user_id }.dump()?;
        }
        Command::Message { room_id, message } => {
            // TODO: Add makro for this
            let config = Config::load()?;
            let client = MnClient::new(&config.user_id).await?;
            if !client.logged_in() {
                eprintln!("not logged in");
                process::exit(1);
            }

            let message = if message == "-" {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                message
            };

            client.sync_once().await?;
            client.send_message(room_id, &message).await?;
        }
        Command::Sync { room_id, raw } => {
            // TODO: Add makro for this
            let config = Config::load()?;
            let client = MnClient::new(&config.user_id).await?;
            if !client.logged_in() {
                eprintln!("not logged in");
                process::exit(1);
            }

            if raw {
                client
                    .sync_with_callback(|r| async move {
                        println!("{}", serde_json::to_string(&r).unwrap());
                        LoopCtrl::Continue
                    })
                    .await?;
                return Ok(());
            }

            client.add_event_handler(
                |event: OriginalSyncRoomMessageEvent, room: Room| async move {
                    let Room::Joined(room) = room else {return};
                    // let MessageType::Text(text_content) = event.content.msgtype else { return };

                    room.read_receipt(&event.event_id).await.unwrap();

                    {
                        #[derive(Serialize)]
                        struct Output {
                            event: OriginalSyncRoomMessageEvent,
                            room_id: String,
                        }

                        let out = Output {
                            event,
                            room_id: room.room_id().to_string(),
                        };
                        println!("{}", serde_json::to_string(&out).unwrap());
                    }

                    // println!("{}|{}|{}", room.room_id(), event.sender, text_content.body);
                },
            );

            client.sync().await?;
        }
    };

    Ok(())
}
