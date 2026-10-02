#!/usr/bin/env bats
# mn clean

setup() {
    load helpers
}

@test "clean deletes the local login, the server session stays" {
    alice=$(new_user alice)
    login a1 "$alice"
    token=$(token_of a1)

    run mn_on a1 clean "$alice"
    assert_success
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/meta.json" ]
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/$alice/store" ]
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/$alice/session.json" ]

    run mn_on a1 whoami
    assert_failure
    assert_output --partial "not logged in"

    # No server call: the token still works.
    run client_api "$token" /account/whoami
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
}

@test "after clean the profile takes a new login" {
    alice=$(new_user alice)
    login a1 "$alice"
    mn_on a1 clean "$alice"

    run login a1 "$alice"
    assert_success
    assert_equal "$(mn_on a1 --json whoami | jq -r .user_id)" "$alice"
}

@test "clean only touches the selected profile" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"
    login_profile a1 bob "$bob"

    run mn_on a1 -p bob clean "$bob"
    assert_success

    run mn_on a1 -p bob whoami
    assert_failure
    assert_equal "$(mn_on a1 --json whoami | jq -r .user_id)" "$alice"
}

@test "clean of a user without local state succeeds" {
    run mn_on a1 clean "@nobody:localhost"
    assert_success
}
