#!/usr/bin/env bats
# mn login, logout and clean.

setup() {
    load helpers
}

@test "login with the password on stdin" {
    alice=$(new_user alice)
    run login a1 "$alice"
    assert_success

    run mn_on a1 --json whoami
    assert_success
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
    assert_equal "$(jq -r .is_guest <<<"$output")" false
}

@test "a wrong password fails and leaves no login behind" {
    alice=$(new_user alice)
    run mn_on a1 login "$alice" --homeserver "$E2E_HOMESERVER" <<<"wrong"
    assert_failure
    assert_output --partial "login failed"

    run mn_on a1 whoami
    assert_failure
    assert_output --partial "not logged in"
}

@test "a second login on the same device is refused" {
    alice=$(new_user alice)
    login a1 "$alice"

    run mn_on a1 login "$alice" --homeserver "$E2E_HOMESERVER" <<<"$(password_of "$alice")"
    assert_failure
    assert_output --partial "mn logout"
}

@test "logout invalidates the token on the server" {
    alice=$(new_user alice)
    login a1 "$alice"
    token=$(mn_on a1 --json homeserver --token --force | jq -r .token)

    run mn_on a1 logout
    assert_success

    run curl --silent --output /dev/null --write-out '%{http_code}' \
        --header "Authorization: Bearer $token" \
        "$E2E_HOMESERVER/_matrix/client/v3/account/whoami"
    assert_output 401

    run mn_on a1 whoami
    assert_failure
    assert_output --partial "not logged in"
}

@test "clean deletes the local login without the server" {
    alice=$(new_user alice)
    login a1 "$alice"

    run mn_on a1 clean "$alice"
    assert_success
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/meta.json" ]

    run mn_on a1 whoami
    assert_failure
}

@test "without a keyring the secrets file is private" {
    alice=$(new_user alice)
    login a1 "$alice"

    run stat --format %a "$BATS_TEST_TMPDIR/a1/mnotify/$alice/session.json"
    assert_output 600
}

@test "two accounts side by side" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"
    login b1 "$bob"

    assert_equal "$(mn_on a1 --json whoami | jq -r .user_id)" "$alice"
    assert_equal "$(mn_on b1 --json whoami | jq -r .user_id)" "$bob"
}
