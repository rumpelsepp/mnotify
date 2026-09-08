use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Context;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::{Client as MatrixClient, SessionTokens};
use rand::distr::{Alphanumeric, SampleString};
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
    state_file(Path::new(user_id.as_str()).join("store"))
}

pub(crate) fn meta_path() -> io::Result<PathBuf> {
    match env::var_os("MN_META_FILE") {
        Some(path) => Ok(path.into()),
        None => state_file("meta.json"),
    }
}

/// Everything that has to survive between invocations and must stay secret: the
/// Matrix session (access token) and the passphrase of the encrypted SQLite
/// store. Kept together in the OS keyring, or -- with `MN_NO_KEYRING` -- in a
/// `0600` JSON file next to the store.
#[derive(Serialize, Deserialize)]
pub(crate) struct Persisted {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session: Option<MatrixSession>,
    pub(crate) store_passphrase: String,
}

impl Persisted {
    fn fresh() -> Self {
        Self {
            session: None,
            store_passphrase: Alphanumeric.sample_string(&mut rand::rng(), 32),
        }
    }
}

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

    fn read(&self) -> anyhow::Result<Option<Persisted>> {
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

    fn write(&self, persisted: &Persisted) -> anyhow::Result<()> {
        let json = serde_json::to_string(persisted)?;
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

    /// Read the stored blob, creating one with a fresh store passphrase when
    /// none exists yet (i.e. before the first login).
    fn read_or_init(&self) -> anyhow::Result<Persisted> {
        match self.read()? {
            Some(persisted) => Ok(persisted),
            None => {
                let persisted = Persisted::fresh();
                self.write(&persisted)?;
                Ok(persisted)
            }
        }
    }
}

/// Load the persisted blob for `user_id`, initialising it (with a fresh store
/// passphrase, no session yet) if this is the first run.
pub(super) fn load_or_init(user_id: &UserId) -> anyhow::Result<Persisted> {
    SessionStore::for_user(user_id)?.read_or_init()
}

/// Overwrite the stored session with the client's current one, keeping the
/// store passphrase. Used as matrix-sdk's save-session callback so a rotated
/// access/refresh token is not lost when the process exits.
pub(super) fn resave_session(user_id: &UserId, client: &MatrixClient) -> anyhow::Result<()> {
    let session = client
        .matrix_auth()
        .session()
        .context("client has no session to save")?;
    let store = SessionStore::for_user(user_id)?;
    let mut persisted = store.read_or_init()?;
    persisted.session = Some(session);
    store.write(&persisted)
}

/// Read the stored token pair back (matrix-sdk's reload-session callback, used
/// when another process refreshed the token first).
pub(super) fn stored_tokens(user_id: &UserId) -> anyhow::Result<SessionTokens> {
    SessionStore::for_user(user_id)?
        .read()?
        .and_then(|p| p.session)
        .map(|s| s.tokens)
        .context("no stored session tokens to reload")
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

    pub(super) fn persist_session(&self) -> anyhow::Result<()> {
        resave_session(&self.user_id, &self.inner)
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
