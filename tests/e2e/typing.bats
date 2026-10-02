#!/usr/bin/env bats
# mn typing

setup() {
    load helpers
}

# Who is typing in $room, as Bob's sync shows it.
typing_users() {
    client_sync "$bob_token" |
        jq -r --arg room "$room" \
            '[.rooms.join[$room].ephemeral.events[]? | select(.type == "m.typing") | .content.user_ids[]] | join(" ")'
}

@test "typing shows the indicator, --disable hides it" {
    alice_and_bob_in_room
    bob_token=$(token_of b1)

    run --separate-stderr mn_on a1 typing --room "$room"
    assert_success
    assert_output ""
    run typing_users
    assert_output "$alice"

    run mn_on a1 typing --room "$room" --disable
    assert_success
    run typing_users
    assert_output ""
}

@test "typing in a room the user is not in fails" {
    alice_and_bob_in_room
    other=$(mn_on a1 room create)

    run mn_on b1 typing --room "$other"
    assert_failure
    assert_output --partial "not a member"
}
