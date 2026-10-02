#!/usr/bin/env bats
# Global options of mn that have no file of their own.

setup() {
    load helpers
}

# Each on a fresh account: Synapse ignores "unavailable" from a sync, and once
# a device synced with "offline", later syncs do not bring it back online.
@test "--presence is what others see" {
    [[ ${MN_SLIDING_SYNC:-} == 0 ]] || skip "Synapse's sliding sync does not set presence"
    alice_and_bob_in_room --public
    alice_token=$(token_of a1)
    presence_of() {
        client_api "$alice_token" "/presence/$1/status" | jq -r .presence
    }

    carol=$(new_user carol)
    login c1 "$carol"
    mn_on c1 --presence offline room join "$room"
    mn_on c1 --presence offline whoami
    assert_equal "$(presence_of "$carol")" offline

    dave=$(new_user dave)
    login d1 "$dave"
    mn_on d1 room join "$room"
    dave_online() { [[ $(presence_of "$dave") == online ]]; }
    run wait_until 10 dave_online
    assert_success
}

@test "--full-state works for one-shot commands" {
    alice_and_bob_in_room
    mn_on a1 send --room "$room" "hi"

    run mn_on b1 --full-state --json messages --room "$room"
    assert_success
    assert_equal "$(last_body)" "hi"
}

@test "logs go to stderr, stdout stays clean" {
    alice_and_bob_in_room

    run --separate-stderr mn_on a1 -vvvv send --room "$room" "hi"
    assert_success
    assert_output --regexp '^\$[^[:space:]]+$'
    assert [ -n "$stderr" ]

    run --separate-stderr mn_on a1 RUST_LOG=debug --json whoami
    assert_success
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
    assert [ -n "$stderr" ]
}

