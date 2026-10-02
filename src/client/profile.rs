//! Profiles: independent sets of local state below `$XDG_STATE_HOME/mnotify/<profile>/`,
//! each with its own login, store and lock. Several accounts, or several
//! devices of one account, can be used side by side this way.

use std::fs::{self, File};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::Context;

use super::CRATE_NAME;

pub(crate) const DEFAULT: &str = "default";

static SELECTED: OnceLock<String> = OnceLock::new();

/// Clap value parser for `--profile`: the name becomes a directory and part
/// of the keyring service name. Names never start with `@`, so they cannot
/// collide with the per-user directories of the layout before profiles.
pub(crate) fn parse_name(name: &str) -> Result<String, String> {
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(name.to_owned())
    } else {
        Err(
            "only ASCII letters, digits, '-', '_' and '.' are allowed, not as the first \
             character '.'"
                .to_owned(),
        )
    }
}

/// Select the profile for the rest of the process; call once, before
/// anything touches the local state.
pub(crate) fn select(name: String) {
    SELECTED
        .set(name)
        .expect("profile must be selected only once");
}

pub(crate) fn name() -> &'static str {
    SELECTED.get().map_or(DEFAULT, String::as_str)
}

/// The state directories of the selected profile.
pub(super) fn dirs() -> xdg::BaseDirectories {
    xdg::BaseDirectories::with_profile(CRATE_NAME, name())
}

/// The keyring service under which the secrets of the selected profile are
/// stored. The default profile keeps the name used before profiles existed,
/// so its secrets stay where they are.
pub(super) fn keyring_service() -> String {
    match name() {
        DEFAULT => CRATE_NAME.to_owned(),
        name => format!("{CRATE_NAME}/{name}"),
    }
}

/// Move the state of an mn from before profiles (`meta.json` and one
/// directory per user directly below `$XDG_STATE_HOME/mnotify/`) into the
/// default profile. Does nothing if there is no such state.
pub(crate) fn migrate_legacy_layout() -> anyhow::Result<()> {
    let Some(root) = xdg::BaseDirectories::with_prefix(CRATE_NAME).get_state_home() else {
        return Ok(());
    };
    if !root.join("meta.json").try_exists()? {
        return Ok(());
    }

    // Two processes started at once must not both move things around.
    let lock_path = root.join(".migrate.lock");
    let lock = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)
        .with_context(|| format!("open {}", lock_path.display()))?;
    lock.lock()
        .with_context(|| format!("lock {}", lock_path.display()))?;

    let result = migrate_locked(&root);
    drop(lock);
    // The lock file is only needed while the old layout is around.
    let _ = fs::remove_file(&lock_path);
    result
}

fn migrate_locked(root: &Path) -> anyhow::Result<()> {
    // Checked again: another process may have migrated while we waited.
    if !root.join("meta.json").try_exists()? {
        return Ok(());
    }
    let target = root.join(DEFAULT);
    if target.join("meta.json").try_exists()? {
        anyhow::bail!(
            "{} and {} both exist; remove one of them",
            root.join("meta.json").display(),
            target.join("meta.json").display()
        );
    }
    fs::create_dir_all(&target)?;

    let mut legacy = Vec::new();
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name();
        if name.to_string_lossy().starts_with('@') {
            legacy.push(name);
        }
    }
    // meta.json last: it marks the old layout, so an interrupted run is
    // picked up again next time.
    legacy.push("meta.json".into());

    for name in legacy {
        let (from, to) = (root.join(&name), target.join(&name));
        if to.try_exists()? {
            anyhow::bail!(
                "cannot move {} into the default profile, {} exists",
                from.display(),
                to.display()
            );
        }
        fs::rename(&from, &to)
            .with_context(|| format!("move {} to {}", from.display(), to.display()))?;
    }

    // Not a log message: it must be visible at the default verbosity, once.
    eprintln!(
        "note: moved the local state of mn from before profiles into the profile \"{DEFAULT}\" ({})",
        target.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{DEFAULT, migrate_locked, parse_name};

    /// The layout before profiles: `meta.json` and one directory per user
    /// directly below the state root.
    fn legacy_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("@bot:example.org");
        fs::create_dir_all(user.join("store")).unwrap();
        fs::write(user.join("session.json"), "secret").unwrap();
        fs::write(root.path().join("meta.json"), "meta").unwrap();
        root
    }

    fn read(path: impl AsRef<Path>) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn profile_names() {
        for ok in ["default", "bot", "work-2", "a_b", "x.y"] {
            assert!(parse_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            ".",
            "..",
            ".hidden",
            "a/b",
            "@bot:example.org",
            "ä",
            "a b",
        ] {
            assert!(parse_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn migration_leaves_other_profiles_alone() {
        let root = legacy_root();
        let root = root.path();
        fs::create_dir_all(root.join("work/@bot:example.org")).unwrap();
        fs::write(root.join("work/meta.json"), "work").unwrap();

        migrate_locked(root).unwrap();

        let default = root.join(DEFAULT);
        assert_eq!(read(default.join("meta.json")), "meta");
        assert_eq!(
            read(default.join("@bot:example.org/session.json")),
            "secret"
        );
        assert!(default.join("@bot:example.org/store").is_dir());
        assert!(!root.join("meta.json").exists());
        assert!(!root.join("@bot:example.org").exists());
        assert_eq!(read(root.join("work/meta.json")), "work");
    }

    #[test]
    fn migration_never_overwrites_a_default_login() {
        let root = legacy_root();
        let root = root.path();
        fs::create_dir_all(root.join(DEFAULT)).unwrap();
        fs::write(root.join(DEFAULT).join("meta.json"), "newer").unwrap();

        let err = migrate_locked(root).unwrap_err().to_string();
        assert!(err.contains("both exist"), "{err}");

        // Nothing has been moved: both logins are still complete.
        assert_eq!(read(root.join("meta.json")), "meta");
        assert_eq!(read(root.join("@bot:example.org/session.json")), "secret");
        assert_eq!(read(root.join(DEFAULT).join("meta.json")), "newer");
    }

    #[test]
    fn interrupted_migration_is_resumed() {
        // A previous run moved the user directory, then died before meta.json.
        let root = legacy_root();
        let root = root.path();
        fs::create_dir_all(root.join(DEFAULT)).unwrap();
        fs::rename(
            root.join("@bot:example.org"),
            root.join(DEFAULT).join("@bot:example.org"),
        )
        .unwrap();

        migrate_locked(root).unwrap();

        assert_eq!(read(root.join(DEFAULT).join("meta.json")), "meta");
        assert!(!root.join("meta.json").exists());
    }

    #[test]
    fn migration_refuses_to_merge_user_directories() {
        let root = legacy_root();
        let root = root.path();
        let target = root.join(DEFAULT).join("@bot:example.org");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("session.json"), "other").unwrap();

        let err = migrate_locked(root).unwrap_err().to_string();
        assert!(err.contains("cannot move"), "{err}");

        assert_eq!(read(root.join("@bot:example.org/session.json")), "secret");
        assert_eq!(read(target.join("session.json")), "other");
        // meta.json still marks the old layout, so the next run tries again.
        assert!(root.join("meta.json").exists());
    }

    #[test]
    fn nothing_to_migrate_without_meta_json() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("@bot:example.org")).unwrap();

        migrate_locked(root.path()).unwrap();

        assert!(root.path().join("@bot:example.org").exists());
        assert!(!root.path().join(DEFAULT).exists());
    }
}
