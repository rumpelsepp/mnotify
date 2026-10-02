#!/usr/bin/env bats
# mn room join

setup() {
    load helpers
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"
    login b1 "$bob"
}

@test "an invited user can join and gets the room ID" {
    room=$(mn_on a1 room create --invite "$bob")

    run --separate-stderr mn_on b1 room join "$room"
    assert_success
    assert_output "$room"

    run mn_on a1 --json room info --room "$room"
    assert_equal "$(jq -r '[.members[].user_id] | sort | join(" ")' <<<"$output")" \
        "$(printf '%s\n' "$alice" "$bob" | sort | paste -sd ' ')"
}

@test "an uninvited user cannot join a private room" {
    room=$(mn_on a1 room create)

    run mn_on b1 room join "$room"
    assert_failure
}

@test "anyone can join a public room, also by alias" {
    alias=${alice#@}
    alias=${alias%%:*}
    room=$(mn_on a1 room create --public --alias "$alias")

    run mn_on b1 room join "#$alias:localhost"
    assert_success
    assert_output "$room"
}

@test "joining twice is fine" {
    room=$(mn_on a1 room create --invite "$bob")
    mn_on b1 room join "$room"

    run mn_on b1 room join "$room"
    assert_success
    assert_output "$room"
}

@test "an unknown alias fails" {
    run mn_on b1 room join "#does-not-exist:localhost"
    assert_failure
}
