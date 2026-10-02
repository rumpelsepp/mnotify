#!/usr/bin/env bats
# mn recovery recover

setup() {
    load helpers
}

# Regression for 8fd50e9: messages from before the login come from the backup.
@test "a second device recovers and reads older messages" {
    alice_and_bob_in_room
    key=$(mn_on a1 recovery enable)
    mn_on b1 send --room "$room" "sent before a2 existed"
    # Only keys one of Alice's devices received end up in her backup.
    mn_on a1 messages --room "$room" >/dev/null

    login a2 "$alice"
    run --separate-stderr mn_on a2 recovery recover <<<"$key"
    assert_success
    assert_output ""

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" true
    assert_equal "$(jq -r .recovery <<<"$output")" Enabled

    run mn_on a2 --json messages --room "$room"
    assert_equal "$(last_body)" "sent before a2 existed"

    # The backup cannot prove who sent it, so it is marked.
    run mn_on a2 messages --room "$room"
    assert_output --partial "[unverified] sent before a2 existed"

    run mn_on a2 send --room "$room" "a2 can send now"
    assert_success
    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_body)" "a2 can send now"
}

@test "the key can be an argument" {
    alice=$(new_user alice)
    login a1 "$alice"
    key=$(mn_on a1 recovery enable)
    login a2 "$alice"

    run mn_on a2 recovery recover "$key"
    assert_success

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" true
}

@test "a malformed or missing key is rejected" {
    alice=$(new_user alice)
    login a1 "$alice"
    mn_on a1 recovery enable >/dev/null
    login a2 "$alice"

    run mn_on a2 recovery recover <<<"not a recovery key"
    assert_failure

    # Nothing on stdin and no terminal: fail, never wait.
    run mn_on a2 recovery recover </dev/null
    assert_failure
    refute [ "$status" -eq 124 ] # 124 = killed by timeout

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" false
}

@test "recover without recovery on the account fails" {
    alice=$(new_user alice)
    login a1 "$alice"
    login a2 "$alice"
    # A well-formed key of another account.
    bob=$(new_user bob)
    login b1 "$bob"
    key=$(mn_on b1 recovery enable)

    run mn_on a2 recovery recover <<<"$key"
    assert_failure
}
