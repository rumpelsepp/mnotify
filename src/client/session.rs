use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;

use anyhow::{anyhow, bail};
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::Session;
use serde::{Deserialize, Serialize};
use tracing::error;

use super::CRATE_NAME;

// The keyring crate uses zbus internally, which prevents it from being used in
// the main context of mnotify (it panics, because a second runtime is created).
// This problem is solved by moving the calls to the keyring crate into an own
// thread.

// TODO: Find out how to wrap a function call properly in rust. :)

macro_rules! error_in_thread {
    ($e:expr, $res:ident) => {
        match $e {
            Ok(e) => e,
            Err(e) => {
                $res = Err(anyhow!("{}", e));
                return;
            }
        }
    };
}

pub(crate) fn session_json_path(user_id: impl AsRef<UserId>) -> anyhow::Result<PathBuf> {
    let user_id = user_id.as_ref();
    let xdg_dirs = xdg::BaseDirectories::with_prefix(CRATE_NAME)?;

    Ok(xdg_dirs.place_state_file(Path::new(&user_id.to_string()).join("session.json"))?)
}

pub(crate) fn state_db_path(user_id: impl AsRef<UserId>) -> anyhow::Result<PathBuf> {
    let user_id = user_id.as_ref();
    let xdg_dirs = xdg::BaseDirectories::with_prefix(CRATE_NAME)?;

    Ok(xdg_dirs.place_state_file(Path::new(&user_id.to_string()).join("state.sled"))?)
}

fn load_session_json(path: impl AsRef<Path>) -> anyhow::Result<Option<Session>> {
    let raw = fs::read_to_string(path)?;
    // TODO: Handle None case.
    Ok(Some(serde_json::from_str(&raw)?))
}

fn load_session_keyring(user_id: impl AsRef<UserId>) -> anyhow::Result<Option<Session>> {
    let mut raw: String = "".into();
    let mut res = Ok(());
    let user_id = user_id.as_ref().to_string();

    thread::scope(|s| {
        let t = s.spawn(|| {
            let entry = error_in_thread!(keyring::Entry::new(CRATE_NAME, user_id.as_str()), res);

            // TODO: Handle None case.
            raw = error_in_thread!(entry.get_password(), res);
        });

        t.join().unwrap();
    });

    Ok(Some(serde_json::from_str(&raw)?))
}

pub(crate) fn load_session(user_id: impl AsRef<UserId>) -> anyhow::Result<Option<Session>> {
    if env::var("MN_NO_KEYRING").is_ok() {
        load_session_json(session_json_path(user_id)?)
    } else {
        load_session_keyring(user_id)
    }
}

fn persist_session_json(path: impl AsRef<Path>, session: &Session) -> anyhow::Result<()> {
    let mut out = serde_json::to_string(session)?;
    if !out.ends_with('\n') {
        out.push('\n');
    }

    fs::write(&path, &out)?;

    let mut perms = fs::metadata(&path)?.permissions();

    let mode = 0o600;
    if perms.mode() != mode {
        perms.set_mode(mode);
    }

    Ok(())
}

fn persist_session_keyring(user_id: impl AsRef<UserId>, session: &Session) -> anyhow::Result<()> {
    let mut res = Ok(());
    let user_id = user_id.as_ref().to_string();

    thread::scope(|s| {
        let t = s.spawn(|| {
            let entry = error_in_thread!(keyring::Entry::new(CRATE_NAME, user_id.as_str()), res);

            let data = error_in_thread!(serde_json::to_string(session), res);
            error_in_thread!(entry.set_password(&data), res);
        });

        t.join().unwrap();
    });

    res
}

pub(crate) fn persist_session(
    user_id: impl AsRef<UserId>,
    session: &Session,
) -> anyhow::Result<()> {
    if env::var("MN_NO_KEYRING").is_ok() {
        persist_session_json(session_json_path(user_id)?, session)
    } else {
        persist_session_keyring(user_id, session)
    }
}

fn delete_session_json(path: impl AsRef<Path>) -> anyhow::Result<()> {
    fs::remove_file(path)?;
    Ok(())
}

fn delete_session_keyring(user_id: impl AsRef<UserId>) -> anyhow::Result<()> {
    let entry = keyring::Entry::new(CRATE_NAME, user_id.as_ref().as_str())?;
    entry.delete_password()?;
    Ok(())
}

pub(crate) fn delete_session(user_id: impl AsRef<UserId>) -> anyhow::Result<()> {
    if env::var("MN_NO_KEYRING").is_ok() {
        delete_session_json(session_json_path(user_id)?)
    } else {
        delete_session_keyring(user_id)
    }
}

pub(crate) fn meta_path() -> io::Result<PathBuf> {
    match env::var("MN_META_FILE") {
        Ok(path) => Ok(path.into()),
        Err(_) => {
            let xdg_dirs = xdg::BaseDirectories::with_prefix(CRATE_NAME)?;
            xdg_dirs.place_state_file("meta.json")
        }
    }
}

impl super::Client {
    pub(crate) fn delete_session(&self) -> anyhow::Result<()> {
        delete_session(&self.user_id)
    }

    pub(crate) fn delete_state_store(&self) -> anyhow::Result<()> {
        fs::remove_dir_all(state_db_path(&self.user_id)?)?;
        Ok(())
    }

    pub(crate) fn clean(&self) -> anyhow::Result<()> {
        if let Err(e) = self.delete_session() {
            error!("delete session: {}", e);
        }
        if let Err(e) = self.delete_state_store() {
            error!("delete state store: {}", e);
        }
        if let Err(e) = fs::remove_file(meta_path()?) {
            error!("delete meta.json: {}", e);
        }
        Ok(())
    }

    pub(super) fn persist_session(&self) -> anyhow::Result<()> {
        let session = self.inner.session().unwrap();
        persist_session(&self.user_id, &session)
    }

    pub(crate) async fn logout(&self) -> anyhow::Result<()> {
        self.inner.logout().await?;
        self.clean()
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Meta {
    pub(crate) user_id: OwnedUserId,
    pub(crate) device_name: Option<String>,
}

impl Meta {
    pub(crate) fn exists() -> io::Result<bool> {
        meta_path()?.try_exists()
    }

    pub(crate) fn load() -> anyhow::Result<Self> {
        let raw = fs::read_to_string(meta_path()?)?;
        if raw.is_empty() {
            bail!("empty file");
        }

        Ok(serde_json::from_str(&raw)?)
    }

    pub(crate) fn dump(&self) -> anyhow::Result<()> {
        let mut raw = serde_json::to_string(&self)?;
        if !raw.ends_with('\n') {
            raw += "\n";
        }
        fs::write(meta_path()?, raw)?;
        Ok(())
    }
}
