#!/usr/bin/env bats
# Profiles: several logins in one state directory, selected with -p/--profile.

setup() {
    load helpers
}

# login_profile DEVICE PROFILE USER: like `login`, into PROFILE of DEVICE.
login_profile() {
    mn_on "$1" -p "$2" login "$3" --homeserver "$E2E_HOMESERVER" --device-name "$1-$2" \
        <<<"$(password_of "$3")" 2>/dev/null
}

@test "two accounts in one state directory" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"
    login_profile a1 bob "$bob"

    assert_equal "$(mn_on a1 --json whoami | jq -r .user_id)" "$alice"
    assert_equal "$(mn_on a1 --json --profile bob whoami | jq -r .user_id)" "$bob"
    assert_equal "$(mn_on a1 MN_PROFILE=bob --json whoami | jq -r .user_id)" "$bob"
    assert [ -e "$BATS_TEST_TMPDIR/a1/mnotify/default/meta.json" ]
    assert [ -e "$BATS_TEST_TMPDIR/a1/mnotify/bob/$bob/session.json" ]
}

@test "a second login is refused per profile, not per state directory" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"

    run mn_on a1 login "$bob" --homeserver "$E2E_HOMESERVER" <<<"$(password_of "$bob")"
    assert_failure
    assert_output --partial "mn -p <name> login"

    run login_profile a1 bob "$bob"
    assert_success
}

@test "an unknown profile is not logged in" {
    alice=$(new_user alice)
    login a1 "$alice"

    run mn_on a1 -p nobody whoami
    assert_failure
    assert_output --partial 'profile "nobody" is not logged in'
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

@test "a second device of the same account sends while sync runs" {
    alice=$(new_user alice)
    login a1 "$alice"
    login_profile a1 sender "$alice"
    room=$(mn_on a1 room create --unencrypted)

    # sync holds the lock of the default profile for as long as it runs.
    E2E_TIMEOUT=40 mn_on a1 --json sync --room "$room" >"$BATS_TEST_TMPDIR/sync.jsonl" 2>/dev/null &
    sync_pid=$!

    # A send that waited for the sync to end would never get through here.
    got_live() {
        E2E_TIMEOUT=15 mn_on a1 -p sender send --room "$room" "live" >/dev/null || return 1
        jq -e 'select(.content.body == "live")' "$BATS_TEST_TMPDIR/sync.jsonl" >/dev/null
    }
    run wait_until 30 got_live
    kill "$sync_pid" 2>/dev/null || true
    assert_success
}

@test "state from before profiles moves into the default profile" {
    alice=$(new_user alice)
    login a1 "$alice"
    # Recreate the old layout: everything directly below mnotify/.
    state=$BATS_TEST_TMPDIR/a1/mnotify
    mv "$state/default/meta.json" "$state/default/$alice" "$state/"
    rmdir "$state/default"

    run --separate-stderr mn_on a1 --json whoami
    assert_success
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
    assert_regex "$stderr" "moved the local state"
    assert [ ! -e "$state/meta.json" ]
    assert [ ! -e "$state/$alice" ]
    assert [ -e "$state/default/$alice/store" ]

    run --separate-stderr mn_on a1 whoami
    assert_success
    refute_regex "$stderr" "moved the local state"
}
