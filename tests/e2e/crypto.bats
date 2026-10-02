#!/usr/bin/env bats
# Cross-signing and MSC4153 behaviour.

setup() {
    load helpers
}

@test "a new account cross-signs its first device" {
    alice=$(new_user alice)
    login a1 "$alice"

    run mn_on a1 --json recovery status
    assert_success
    assert_equal "$(jq -r .cross_signing_complete <<<"$output")" true
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" true
}

@test "a second device is not cross-signed and cannot send encrypted" {
    alice_and_bob_in_room

    run mn_on a2 login "$alice" --homeserver "$E2E_HOMESERVER" <<<"$(password_of "$alice")"
    assert_success
    assert_output --partial "not cross-signed"

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" false

    run mn_on a2 send --room "$room" "from a2"
    assert_failure
    assert_output --partial "not cross-signed"
}

@test "an unencrypted room works without cross-signing" {
    alice_and_bob_in_room --unencrypted
    login a2 "$alice"

    run mn_on a2 send --room "$room" "plain"
    assert_success
}
