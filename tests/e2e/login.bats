#!/usr/bin/env bats
# mn login

setup() {
    load helpers
}

@test "login with the password on stdin" {
    alice=$(new_user alice)
    run --separate-stderr mn_on a1 login "$alice" --homeserver "$E2E_HOMESERVER" \
        <<<"$(password_of "$alice")"
    assert_success
    assert_output ""

    run mn_on a1 --json whoami
    assert_success
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
}

@test "a wrong password fails and leaves no login behind" {
    alice=$(new_user alice)
    run mn_on a1 login "$alice" --homeserver "$E2E_HOMESERVER" <<<"wrong"
    assert_failure
    assert_output --partial "login failed"
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/meta.json" ]

    run mn_on a1 whoami
    assert_failure
    assert_output --partial "not logged in"

    # The failed attempt is no obstacle for the next one.
    run login a1 "$alice"
    assert_success
}

@test "an unreachable homeserver fails and leaves no login behind" {
    alice=$(new_user alice)
    run mn_on a1 login "$alice" --homeserver http://127.0.0.1:9 <<<"$(password_of "$alice")"
    assert_failure
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/meta.json" ]
}

@test "a second login is refused per profile, not per state directory" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"

    run mn_on a1 login "$bob" --homeserver "$E2E_HOMESERVER" <<<"$(password_of "$bob")"
    assert_failure
    assert_output --partial "already logged in as $alice"
    assert_output --partial "mn -p <name> login"
    assert_equal "$(mn_on a1 --json whoami | jq -r .user_id)" "$alice"

    run login_profile a1 bob "$bob"
    assert_success
}

@test "the device name is what other clients see" {
    alice=$(new_user alice)
    mn_on a1 login "$alice" --homeserver "$E2E_HOMESERVER" --device-name "backup on nas" \
        <<<"$(password_of "$alice")" 2>/dev/null
    device=$(mn_on a1 --json whoami | jq -r .device_id)

    run client_api "$(token_of a1)" "/devices/$device"
    assert_equal "$(jq -r .display_name <<<"$output")" "backup on nas"
}

@test "without a keyring the secrets file is private, and login says so" {
    alice=$(new_user alice)
    run --separate-stderr mn_on a1 login "$alice" --homeserver "$E2E_HOMESERVER" \
        <<<"$(password_of "$alice")"
    secrets=$BATS_TEST_TMPDIR/a1/mnotify/default/$alice/session.json
    assert_regex "$stderr" "kept in plain text in $secrets"

    run stat --format %a "$secrets"
    assert_output 600
}

@test "a new account cross-signs its first device and hints at recovery" {
    alice=$(new_user alice)
    run --separate-stderr mn_on a1 login "$alice" --homeserver "$E2E_HOMESERVER" \
        <<<"$(password_of "$alice")"
    assert_regex "$stderr" "mn recovery enable"
    refute_regex "$stderr" "not cross-signed"

    run mn_on a1 --json recovery status
    assert_equal "$(jq -r .cross_signing_complete <<<"$output")" true
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" true
}

@test "a further device of a cross-signed account is not cross-signed" {
    alice=$(new_user alice)
    login a1 "$alice"

    run mn_on a2 login "$alice" --homeserver "$E2E_HOMESERVER" <<<"$(password_of "$alice")"
    assert_success
    assert_output --partial "not cross-signed"

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" false
}

@test "MN_SLIDING_SYNC=0 at login sticks to /v3/sync" {
    alice=$(new_user alice)
    mn_on a1 MN_SLIDING_SYNC=0 login "$alice" --homeserver "$E2E_HOMESERVER" \
        <<<"$(password_of "$alice")" 2>/dev/null

    run jq -r .sliding_sync "$BATS_TEST_TMPDIR/a1/mnotify/default/meta.json"
    assert_output false
}
