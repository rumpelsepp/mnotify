#!/usr/bin/env bats
# mn -p/--profile: several logins in one state directory.

setup() {
    load helpers
}

@test "two accounts in one state directory" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"
    login_profile a1 bob "$bob"

    assert_equal "$(mn_on a1 --json whoami | jq -r .user_id)" "$alice"
    assert_equal "$(mn_on a1 --json -p default whoami | jq -r .user_id)" "$alice"
    assert_equal "$(mn_on a1 --json --profile bob whoami | jq -r .user_id)" "$bob"
    assert_equal "$(mn_on a1 MN_PROFILE=bob --json whoami | jq -r .user_id)" "$bob"
    # -p wins over MN_PROFILE.
    assert_equal "$(mn_on a1 MN_PROFILE=bob --json -p default whoami | jq -r .user_id)" "$alice"
    assert [ -e "$BATS_TEST_TMPDIR/a1/mnotify/default/meta.json" ]
    assert [ -e "$BATS_TEST_TMPDIR/a1/mnotify/bob/$bob/session.json" ]
}

@test "an unknown profile is not logged in" {
    alice=$(new_user alice)
    login a1 "$alice"

    run mn_on a1 -p nobody whoami
    assert_failure
    assert_output --partial 'profile "nobody" is not logged in'
}

@test "invalid profile names are refused" {
    for name in ".hidden" "a/b" "" "with space"; do
        run mn_on a1 -p "$name" whoami
        assert_failure
        assert_output --partial "profile"
    done
    assert [ ! -e "$BATS_TEST_TMPDIR/a1/mnotify/.hidden" ]
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
