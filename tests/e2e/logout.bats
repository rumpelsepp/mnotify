#!/usr/bin/env bats
# mn logout

setup() {
    load helpers
}

@test "logout invalidates the token and deletes the local state" {
    alice=$(new_user alice)
    login a1 "$alice"
    token=$(token_of a1)

    run mn_on a1 logout
    assert_success

    run client_api "$token" /account/whoami --output /dev/null --write-out '%{http_code}'
    assert_output 401
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/meta.json" ]
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/$alice/store" ]
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/default/$alice/session.json" ]

    run mn_on a1 whoami
    assert_failure
    assert_output --partial "not logged in"
}

@test "logout without a login fails" {
    run mn_on a1 logout
    assert_failure
    assert_output --partial "not logged in"
}

@test "after logout the same account logs in again as a new device" {
    alice=$(new_user alice)
    login a1 "$alice"
    old=$(mn_on a1 --json whoami | jq -r .device_id)
    mn_on a1 logout

    run login a1 "$alice"
    assert_success
    refute [ "$(mn_on a1 --json whoami | jq -r .device_id)" = "$old" ]
}

@test "logout in one profile keeps the others" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"
    login_profile a1 bob "$bob"

    run mn_on a1 -p bob logout
    assert_success

    run mn_on a1 -p bob whoami
    assert_failure
    assert_equal "$(mn_on a1 --json whoami | jq -r .user_id)" "$alice"
}
