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
    for extra in [["--qr"].as_slice(), ["--sso"].as_slice()] {
        let mut cmd = mn();
        cmd.args(["login", "@user:example.org", "--password", "hunter2"])
            .args(extra)
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

#[test]
fn idp_requires_sso() {
    mn().args(["login", "@user:example.org", "--idp", "saml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--sso"));
}
