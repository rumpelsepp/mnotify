#!/usr/bin/env bats
# mn room create

setup() {
    load helpers
    alice=$(new_user alice)
    login a1 "$alice"
}

@test "a new room is encrypted, private and named as asked" {
    run --separate-stderr mn_on a1 room create --name "Ops" --topic "alerts"
    assert_success
    assert_output --regexp '^![^[:space:]]+$'
    room=$output

    run mn_on a1 --json room info --room "$room"
    assert_success
    assert_equal "$(jq -r .is_encrypted <<<"$output")" true
    assert_equal "$(jq -r .is_public <<<"$output")" false
    assert_equal "$(jq -r .name <<<"$output")" "Ops"
    assert_equal "$(jq -r .topic <<<"$output")" "alerts"
    assert_equal "$(jq -r '[.members[].user_id] | join(" ")' <<<"$output")" "$alice"
}

@test "--unencrypted creates a plain room" {
    room=$(mn_on a1 room create --unencrypted)

    run mn_on a1 --json room info --room "$room"
    assert_equal "$(jq -r .is_encrypted <<<"$output")" false
}

@test "--invite invites every user given" {
    bob=$(new_user bob)
    carol=$(new_user carol)
    login b1 "$bob"
    login c1 "$carol"
    room=$(mn_on a1 room create --invite "$bob" --invite "$carol")

    run mn_on b1 room join "$room"
    assert_success
    run mn_on c1 room join "$room"
    assert_success
}

@test "--public lets anyone join" {
    room=$(mn_on a1 room create --public)

    run mn_on a1 --json room info --room "$room"
    assert_equal "$(jq -r .is_public <<<"$output")" true
    # Joining is tested in room-join.bats.
}

@test "--alias publishes the alias, a taken alias fails" {
    alias=${alice#@}
    alias=${alias%%:*}
    room=$(mn_on a1 room create --alias "$alias")

    run mn_on a1 --json room info --room "#$alias:localhost"
    assert_equal "$(jq -r .room_id <<<"$output")" "$room"

    run mn_on a1 room create --alias "$alias"
    assert_failure
}
