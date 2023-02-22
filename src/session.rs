use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use matrix_sdk::ruma::UserId;
use matrix_sdk::Session;

use super::CRATE_NAME;

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
    let entry = keyring::Entry::new(CRATE_NAME, user_id.as_ref().as_str());
    // TODO: Handle None case.
    let raw = entry.get_password()?;
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

    fs::write(path, &out)?;

    let mut perms = fs::metadata(&out)?.permissions();

    let mode = 0o600;
    if perms.mode() != mode {
        perms.set_mode(mode);
    }

    Ok(())
}

fn persist_session_keyring(user_id: impl AsRef<UserId>, session: &Session) -> anyhow::Result<()> {
    let entry = keyring::Entry::new(CRATE_NAME, user_id.as_ref().as_str());
    entry.set_password(&serde_json::to_string(session)?)?;
    Ok(())
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
    let entry = keyring::Entry::new(CRATE_NAME, user_id.as_ref().as_str());
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
