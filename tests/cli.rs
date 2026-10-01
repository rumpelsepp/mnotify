//! CLI surface tests. These only exercise argument parsing -- they never touch
//! the network, the keyring or the state store -- so they run anywhere without
//! setup.

use assert_cmd::Command;
use predicates::prelude::*;

fn mn() -> Command {
    Command::cargo_bin("mn").expect("the `mn` binary is built")
}

#[test]
fn no_subcommand_prints_usage() {
    mn().assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn unknown_subcommand_fails() {
    mn().arg("does-not-exist").assert().failure().stderr(
        predicate::str::contains("unrecognized subcommand").or(predicate::str::contains("Usage:")),
    );
}

#[test]
fn help_lists_the_commands() {
    mn().arg("--help").assert().success().stdout(
        predicate::str::contains("send")
            .and(predicate::str::contains("recovery"))
            .and(predicate::str::contains("verify")),
    );
}

#[test]
fn version_matches_the_crate() {
    mn().arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn send_requires_a_room() {
    mn().arg("send")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--room-id").or(predicate::str::contains("required")));
}

#[test]
fn login_methods_are_mutually_exclusive() {
    mn().args(["login", "@user:example.org", "--qr", "--sso"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn passwords_are_not_taken_from_the_command_line() {
    mn().args(["login", "@user:example.org", "--password", "hunter2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn idp_requires_sso() {
    mn().args(["login", "@user:example.org", "--idp", "saml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--sso"));
}

#[test]
fn send_accepts_ids_and_aliases_and_mn_room() {
    mn().args(["send", "--help"]).assert().success().stdout(
        predicate::str::contains("#ops:example.org").and(predicate::str::contains("MN_ROOM")),
    );
}

#[test]
fn room_without_sigil_is_rejected() {
    mn().args(["send", "-r", "ops", "hi"])
        .env_remove("MN_ROOM")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--room"));
}

#[test]
fn attachment_cannot_be_a_notice() {
    mn().args(["send", "-r", "#ops:example.org", "-n", "-a", "x.png"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

/// Point all state at an empty temp dir so nothing real is touched.
fn isolated(dir: &std::path::Path) -> Command {
    let mut cmd = mn();
    cmd.env("XDG_STATE_HOME", dir)
        .env_remove("MN_META_FILE")
        .env_remove("MN_ROOM")
        .env("MN_NO_KEYRING", "1");
    cmd
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mn-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn not_logged_in_says_how_to_log_in() {
    let dir = temp_dir("no-login");
    isolated(&dir)
        .args(["send", "-r", "#ops:example.org", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mn login"));
}

#[test]
fn clean_works_offline() {
    let dir = temp_dir("clean");
    isolated(&dir)
        .args(["clean", "@nobody:invalid.example"])
        .assert()
        .success();
}

#[test]
fn falls_back_to_a_private_file_without_keyring() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("keyring-fallback");
    // No MN_NO_KEYRING, and an unreachable session bus so that a real
    // keyring on the test machine is never touched.
    isolated(&dir)
        .env_remove("MN_NO_KEYRING")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/bus")
        .args(["login", "@bot:mn-test.invalid"])
        .write_stdin("hunter2\n")
        .timeout(std::time::Duration::from_secs(60))
        .assert()
        .failure(); // the homeserver does not exist, but the secrets file is set up first

    let session = dir.join("mnotify/@bot:mn-test.invalid/session.json");
    let mode = std::fs::metadata(&session).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn thread_and_reply_are_exclusive() {
    let event = "$abc:example.org";
    mn().args(["send", "-r", "#ops:example.org", "--thread", event])
        .args(["--reply-to", event, "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn messages_can_read_a_thread() {
    mn().args(["messages", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--thread"));
}

#[test]
fn mentions_need_full_user_ids() {
    mn().args(["send", "-r", "#ops:example.org", "--mention", "alice", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--mention"));
}

#[test]
fn json_flag_works_after_the_subcommand() {
    mn().args(["rooms", "--json", "--help"]).assert().success();
}
