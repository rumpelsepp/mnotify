#!/usr/bin/env bats
# mn room create, join and rooms.

setup() {
    load helpers
    alice=$(new_user alice)
    login a1 "$alice"
}

@test "a new room is encrypted by default" {
    run mn_on a1 room create --name "Ops" --topic "alerts"
    assert_success
    assert_output --regexp '^![^[:space:]]+$'
    room=$output

    run mn_on a1 --json room info --room "$room"
    assert_success
    assert_equal "$(jq -r .is_encrypted <<<"$output")" true
    assert_equal "$(jq -r .name <<<"$output")" "Ops"
    assert_equal "$(jq -r .topic <<<"$output")" "alerts"
}

@test "--unencrypted creates a plain room" {
    room=$(mn_on a1 room create --unencrypted)

    run mn_on a1 --json room info --room "$room"
    assert_equal "$(jq -r .is_encrypted <<<"$output")" false
}

@test "an invited user can join" {
    bob=$(new_user bob)
    login b1 "$bob"
    room=$(mn_on a1 room create --invite "$bob")

    run mn_on b1 room join "$room"
    assert_success
    assert_output "$room"
}

@test "an uninvited user cannot join a private room" {
    bob=$(new_user bob)
    login b1 "$bob"
    room=$(mn_on a1 room create)

    run mn_on b1 room join "$room"
    assert_failure
}

@test "anyone can join a public room" {
    bob=$(new_user bob)
    login b1 "$bob"
    room=$(mn_on a1 room create --public)

    run mn_on b1 room join "$room"
    assert_success
}

@test "the alias works wherever a room is expected" {
    alias=${alice#@}
    alias=${alias%%:*}
    room=$(mn_on a1 room create --alias "$alias")

    run mn_on a1 send --room "#$alias:localhost" "via alias"
    assert_success

    run mn_on a1 --json messages --room "$room"
    assert_equal "$(last_body)" "via alias"
}

@test "creating a room with a taken alias fails" {
    alias=${alice#@}
    alias=${alias%%:*}
    mn_on a1 room create --alias "$alias"

    run mn_on a1 room create --alias "$alias"
    assert_failure
}
