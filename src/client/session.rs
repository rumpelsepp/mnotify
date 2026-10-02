use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::Context;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::authentication::oauth::{ClientId, OAuthSession, UserSession};
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::{AuthSession, Client as MatrixClient, SessionTokens};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use super::CRATE_NAME;

/// A persisted Matrix session, from either authentication API. `OAuthSession`
/// itself is not `Serialize`, so its two parts are stored separately.
#[derive(Serialize, Deserialize)]
pub(crate) enum StoredSession {
    Matrix(MatrixSession),
    OAuth {
        client_id: ClientId,
        user: UserSession,
    },
}

impl StoredSession {
    fn from_client(client: &MatrixClient) -> Option<Self> {
        Some(match client.session()? {
            AuthSession::Matrix(session) => Self::Matrix(session),
            AuthSession::OAuth(session) => Self::OAuth {
                client_id: session.client_id,
                user: session.user,
            },
            _ => return None,
        })
    }

    fn tokens(&self) -> SessionTokens {
        match self {
            Self::Matrix(session) => session.tokens.clone(),
            Self::OAuth { user, .. } => user.tokens.clone(),
        }
    }
}

impl From<StoredSession> for AuthSession {
    fn from(session: StoredSession) -> Self {
        match session {
            StoredSession::Matrix(session) => Self::Matrix(session),
            StoredSession::OAuth { client_id, user } => {
                Self::OAuth(Box::new(OAuthSession { client_id, user }))
            }
        }
    }
}

/// Write `data` to `path` atomically, readable by the owner only from the
/// first byte on: a temp file created with mode 0600, then renamed over the
/// target. Concurrent readers see either the old or the new content.
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    fs::rename(&tmp, path)
}

pub(super) fn state_file(relative: impl AsRef<Path>) -> io::Result<PathBuf> {
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
    pub(crate) session: Option<StoredSession>,
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

fn keyring_error(e: keyring::Error) -> anyhow::Error {
    anyhow::Error::new(e).context("system keyring (Secret Service) error")
}

/// An entry in the system keyring, if the keyring can actually be used: the
/// store initialises (fails without a session bus, i.e. on most servers) and
/// a lookup does not fail (fails without a running Secret Service).
fn usable_keyring_entry(user_id: &UserId) -> keyring::Result<keyring::Entry> {
    let entry = keyring::Entry::new(CRATE_NAME, user_id.as_str())?;
    match entry.get_password() {
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(entry),
        Err(e) => Err(e),
    }
}

enum SessionStore {
    Keyring(keyring::Entry),
    File(PathBuf),
}

impl SessionStore {
    /// Where the secrets of `user_id` live. Once a `session.json` exists it
    /// is always used, so the choice is stable across invocations. Before
    /// that, the system keyring is preferred and the file is the automatic
    /// fallback on machines without one.
    fn for_user(user_id: &UserId) -> anyhow::Result<Self> {
        let path = session_json_path(user_id)?;
        if env::var_os("MN_NO_KEYRING").is_some() || path.try_exists()? {
            return Ok(Self::File(path));
        }

        match usable_keyring_entry(user_id) {
            Ok(entry) => Ok(Self::Keyring(entry)),
            // A store without a session file means the secrets went into a
            // keyring that is not reachable now (e.g. logged in from the
            // desktop, running from cron). A fresh passphrase would make
            // the existing store unreadable, so stop here.
            Err(e) if state_db_path(user_id)?.try_exists()? => {
                Err(keyring_error(e).context(format!(
                    "the secrets of {user_id} are in the system keyring, which is \
                     not reachable from here; run mn where the keyring is \
                     unlocked, or start over with `mn clean {user_id}` and log in again"
                )))
            }
            Err(e) => {
                debug!("no usable system keyring ({e}), using {}", path.display());
                Ok(Self::File(path))
            }
        }
    }

    fn read(&self) -> anyhow::Result<Option<Persisted>> {
        let raw = match self {
            Self::Keyring(entry) => match entry.get_password() {
                Ok(raw) => raw,
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(e) => return Err(keyring_error(e)),
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
            Self::Keyring(entry) => entry.set_password(&json).map_err(keyring_error)?,
            Self::File(path) => write_private(path, json.as_bytes())?,
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

/// The plain file holding the secrets of `user_id`, or `None` if they are in
/// the system keyring.
pub(crate) fn secrets_file(user_id: &UserId) -> anyhow::Result<Option<PathBuf>> {
    Ok(match SessionStore::for_user(user_id)? {
        SessionStore::File(path) => Some(path),
        SessionStore::Keyring(_) => None,
    })
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
    let session = StoredSession::from_client(client).context("client has no session to save")?;
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
        .map(|s| s.tokens())
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

/// Delete the session secrets, the state store and `meta.json` of `user_id`,
/// logging (but not failing on) each individual error. Purely local: works
/// offline and without a usable session.
pub(crate) fn clean(user_id: &UserId) {
    for (what, result) in [
        (
            "session",
            SessionStore::for_user(user_id).and_then(|s| s.delete()),
        ),
        ("state store", remove_state_db(user_id)),
        ("meta.json", remove_meta()),
    ] {
        if let Err(e) = result {
            error!("delete {what}: {e:#}");
        }
    }
}

impl super::Client {
    pub(super) fn persist_session(&self) -> anyhow::Result<()> {
        resave_session(&self.user_id, &self.inner)
    }

    pub(crate) async fn logout(&self) -> anyhow::Result<()> {
        // Dispatches to the legacy Matrix or the OAuth 2.0 logout, depending
        // on how we logged in (`matrix_auth()` alone fails for --qr sessions).
        self.inner.logout().await?;
        clean(&self.user_id);
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Meta {
    pub(crate) user_id: OwnedUserId,
    pub(crate) device_name: String,
    /// Homeserver URL found at login, so later runs skip the discovery.
    pub(crate) homeserver: String,
    /// Whether this login syncs via sliding sync. Fixed at login: both APIs
    /// keep their to-device token in the same place in the crypto store, and
    /// a token of the one is rejected by the other.
    pub(crate) sliding_sync: bool,
}

impl Meta {
    pub(crate) fn exists() -> io::Result<bool> {
        meta_path()?.try_exists()
    }

    pub(crate) fn load() -> anyhow::Result<Self> {
        let raw = fs::read_to_string(meta_path()?)?;
        serde_json::from_str(&raw).map_err(|e| {
            // Most likely written by an older mn, which lacked some fields.
            let user = serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|v| v["user_id"].as_str().map(str::to_owned))
                .unwrap_or_else(|| "@user:example.org".to_owned());
            anyhow::anyhow!(
                "unsupported meta.json ({e}), probably from an older mn; \
                 remove the login with `mn clean {user}` and log in again"
            )
        })
    }

    pub(crate) fn dump(&self) -> anyhow::Result<()> {
        fs::write(meta_path()?, format!("{}\n", serde_json::to_string(self)?))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Meta;

    #[test]
    fn meta_from_older_versions_is_rejected() {
        let old = r#"{"user_id":"@bot:example.org","device_name":null}"#;
        assert!(serde_json::from_str::<Meta>(old).is_err());
    }
}
