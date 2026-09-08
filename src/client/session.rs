use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Context;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use serde::{Deserialize, Serialize};
use tracing::error;

use super::CRATE_NAME;

fn state_file(relative: impl AsRef<Path>) -> io::Result<PathBuf> {
    xdg::BaseDirectories::with_prefix(CRATE_NAME).place_state_file(relative)
}

fn session_json_path(user_id: &UserId) -> io::Result<PathBuf> {
    state_file(Path::new(user_id.as_str()).join("session.json"))
}

pub(crate) fn state_db_path(user_id: &UserId) -> io::Result<PathBuf> {
    state_file(Path::new(user_id.as_str()).join("state.sled"))
}

pub(crate) fn meta_path() -> io::Result<PathBuf> {
    match env::var_os("MN_META_FILE") {
        Some(path) => Ok(path.into()),
        None => state_file("meta.json"),
    }
}

/// Where the Matrix session (i.e. the access token) is kept.
///
/// The OS keyring is used by default; setting `MN_NO_KEYRING` falls back to a
/// `0600` JSON file next to the state store.
enum SessionStore {
    Keyring(keyring::Entry),
    File(PathBuf),
}

impl SessionStore {
    fn for_user(user_id: &UserId) -> anyhow::Result<Self> {
        if env::var_os("MN_NO_KEYRING").is_some() {
            Ok(Self::File(session_json_path(user_id)?))
        } else {
            Ok(Self::Keyring(keyring::Entry::new(
                CRATE_NAME,
                user_id.as_str(),
            )?))
        }
    }

    fn load(&self) -> anyhow::Result<Option<MatrixSession>> {
        let raw = match self {
            Self::Keyring(entry) => match entry.get_password() {
                Ok(raw) => raw,
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(e) => return Err(e.into()),
            },
            Self::File(path) => match fs::read_to_string(path) {
                Ok(raw) => raw,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            },
        };
        Ok(Some(serde_json::from_str(&raw)?))
    }

    fn persist(&self, session: &MatrixSession) -> anyhow::Result<()> {
        let json = serde_json::to_string(session)?;
        match self {
            Self::Keyring(entry) => entry.set_password(&json)?,
            Self::File(path) => {
                fs::write(path, &json)?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            }
        }
        Ok(())
    }

    fn delete(&self) -> anyhow::Result<()> {
        match self {
            Self::Keyring(entry) => entry.delete_credential()?,
            Self::File(path) => fs::remove_file(path)?,
        }
        Ok(())
    }
}

fn remove_state_db(user_id: &UserId) -> anyhow::Result<()> {
    fs::remove_dir_all(state_db_path(user_id)?)?;
    Ok(())
}

fn remove_meta() -> anyhow::Result<()> {
    fs::remove_file(meta_path()?)?;
    Ok(())
}

impl super::Client {
    fn session_store(&self) -> anyhow::Result<SessionStore> {
        SessionStore::for_user(&self.user_id)
    }

    pub(super) fn load_session(&self) -> anyhow::Result<Option<MatrixSession>> {
        self.session_store()?.load()
    }

    pub(super) fn persist_session(&self) -> anyhow::Result<()> {
        let session = self
            .inner
            .matrix_auth()
            .session()
            .context("no Matrix session to persist")?;
        self.session_store()?.persist(&session)
    }

    /// Delete the session, the state store and `meta.json`, logging (but not
    /// failing on) each individual error.
    pub(crate) fn clean(&self) -> anyhow::Result<()> {
        for (what, result) in [
            ("session", self.session_store().and_then(|s| s.delete())),
            ("state store", remove_state_db(&self.user_id)),
            ("meta.json", remove_meta()),
        ] {
            if let Err(e) = result {
                error!("delete {what}: {e}");
            }
        }
        Ok(())
    }

    pub(crate) async fn logout(&self) -> anyhow::Result<()> {
        self.inner.matrix_auth().logout().await?;
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
        anyhow::ensure!(!raw.trim().is_empty(), "meta.json is empty");
        Ok(serde_json::from_str(&raw)?)
    }

    pub(crate) fn dump(&self) -> anyhow::Result<()> {
        fs::write(meta_path()?, format!("{}\n", serde_json::to_string(self)?))?;
        Ok(())
    }
}
