#!/usr/bin/env bats
# mn room list

setup() {
    load helpers
}

@test "a new account has no rooms" {
    alice=$(new_user alice)
    login a1 "$alice"

    run --separate-stderr mn_on a1 --json room list
    assert_success
    assert_output "[]"
}

@test "lists the joined rooms with their details, sorted by name" {
    alice_and_bob_in_room --name "zeta"
    plain=$(mn_on a1 room create --name "alpha" --unencrypted)

    run mn_on a1 --json room list
    assert_success
    assert_equal "$(jq -r '[.[].room_id] | join(" ")' <<<"$output")" "$plain $room"
    assert_equal "$(jq -r '[.[].is_encrypted] | join(" ")' <<<"$output")" "false true"
    assert_equal "$(jq -r '[.[].members | length] | join(" ")' <<<"$output")" "1 2"

    run mn_on a1 room list
    assert_line --index 0 --regexp '^ *NAME +ROOM ID +MEMBERS +ENCRYPTED +UNREAD *$'
    assert_line --index 1 --regexp "^ *alpha +$plain +1 +no +0 *\$"
    assert_line --index 2 --regexp "^ *zeta +$room +2 +yes +0 *\$"
}

