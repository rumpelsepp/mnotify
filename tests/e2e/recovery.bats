#!/usr/bin/env bats
# Key backup and recovery.

setup() {
    load helpers
}

@test "enable prints a recovery key and turns on the backup" {
    alice=$(new_user alice)
    login a1 "$alice"

    run --separate-stderr mn_on a1 recovery enable
    assert_success
    assert_output --regexp '^([1-9A-HJ-NP-Za-km-z]{4} ){11}[1-9A-HJ-NP-Za-km-z]{4}$'

    run mn_on a1 --json recovery status
    assert_equal "$(jq -r .recovery <<<"$output")" Enabled
    assert_equal "$(jq -r .backup_on_server <<<"$output")" true
}

@test "enable twice is refused" {
    alice=$(new_user alice)
    login a1 "$alice"
    mn_on a1 recovery enable >/dev/null

    run mn_on a1 recovery enable
    assert_failure
    assert_output --partial "already enabled"
}

# Regression for 8fd50e9: messages from before the login come from the backup.
@test "a second device recovers and reads older messages" {
    alice_and_bob_in_room
    key=$(mn_on a1 recovery enable)
    mn_on b1 send --room "$room" "sent before a2 existed"
    # Only keys one of Alice's devices received end up in her backup.
    mn_on a1 messages --room "$room" >/dev/null

    login a2 "$alice"
    run mn_on a2 recovery recover <<<"$key"
    assert_success

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" true

    run mn_on a2 --json messages --room "$room"
    assert_equal "$(last_body)" "sent before a2 existed"

    # The backup cannot prove who sent it, so it is marked.
    run mn_on a2 messages --room "$room"
    assert_output --partial "[unverified] sent before a2 existed"

    run mn_on a2 send --room "$room" "a2 can send now"
    assert_success
}

@test "a malformed recovery key is rejected" {
    alice=$(new_user alice)
    login a1 "$alice"
    mn_on a1 recovery enable >/dev/null
    login a2 "$alice"

    run mn_on a2 recovery recover <<<"not a recovery key"
    assert_failure

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" false
}

@test "reset replaces the key: the old one stops working" {
    alice=$(new_user alice)
    login a1 "$alice"
    old=$(mn_on a1 recovery enable)

    run --separate-stderr mn_on a1 recovery reset
    assert_success
    new=$output
    refute [ "$old" = "$new" ]

    login a2 "$alice"
    run mn_on a2 recovery recover <<<"$old"
    assert_failure
    run mn_on a2 recovery recover <<<"$new"
    assert_success
}

@test "disable deletes the backup on the server" {
    alice=$(new_user alice)
    login a1 "$alice"
    mn_on a1 recovery enable >/dev/null

    run mn_on a1 recovery disable
    assert_success

    run mn_on a1 --json recovery status
    assert_equal "$(jq -r .backup_on_server <<<"$output")" false
}
