use std::fs::{self, File, TryLockError};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use matrix_sdk::ruma::UserId;
use tracing::{info, warn};

use super::session::state_file;

/// After this long, waiting for the lock is logged as a warning instead of
/// an info message: the other process may be a `mn sync` that never ends.
const WARN_AFTER: Duration = Duration::from_secs(10);

/// An exclusive lock on the local state of one account in the selected
/// profile, held until dropped (i.e. until the process exits). Other profiles,
/// even of the same account, have their own store and lock.
///
/// The crypto store must not be used by two processes at once: each one
/// loads the Olm sessions into memory, and two of them encrypting with the
/// same session produce two messages with the same ratchet index. The
/// receiver can decrypt only the first; the room key in the second is lost
/// and every message encrypted with it stays undecryptable. matrix-sdk's
/// cross-process store lock covers only a few code paths, so concurrent `mn`
/// invocations on the same store are serialized as a whole instead.
pub(crate) struct AccountLock {
    _file: File,
}

impl AccountLock {
    /// Take the lock of `user_id`, waiting as long as another process holds it.
    pub(crate) async fn acquire(user_id: &UserId) -> anyhow::Result<Self> {
        // Next to the store, not in it: `mn clean` deletes the store, and a
        // new lock file would let a waiting process run beside a new one.
        let path = state_file(Path::new(user_id.as_str()).join("lock"))?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("open lock file {}", path.display()))?;

        let file = match file.try_lock() {
            Ok(()) => file,
            Err(TryLockError::WouldBlock) => wait(file, &path, user_id).await?,
            Err(TryLockError::Error(e)) => {
                return Err(e).with_context(|| format!("lock {}", path.display()));
            }
        };

        // Only for the message of the next process that has to wait.
        file.set_len(0)?;
        writeln!(&file, "{}", std::process::id())?;
        Ok(Self { _file: file })
    }
}

async fn wait(file: File, path: &Path, user_id: &UserId) -> anyhow::Result<File> {
    let holder = fs::read_to_string(path).unwrap_or_default();
    let holder = match holder.trim() {
        "" => "another mn process".to_owned(),
        pid => format!("mn process {pid}"),
    };
    info!("waiting for {holder}, which uses the local state of {user_id}");

    let mut locked = tokio::task::spawn_blocking(move || file.lock().map(|()| file));
    let result = match tokio::time::timeout(WARN_AFTER, &mut locked).await {
        Ok(result) => result,
        Err(_) => {
            warn!(
                "still waiting for {holder}, which uses the local state of {user_id}; \
                 only one mn process per account can run at a time"
            );
            locked.await
        }
    };
    result?.with_context(|| format!("lock {}", path.display()))
}
