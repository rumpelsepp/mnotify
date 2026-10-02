#!/usr/bin/env bats
# mn room info

setup() {
    load helpers
}

@test "info shows the room's details and members" {
    alice_and_bob_in_room --name "Ops" --topic "alerts"

    run --separate-stderr mn_on b1 --json room info --room "$room"
    assert_success
    assert_equal "$(jq -r .room_id <<<"$output")" "$room"
    assert_equal "$(jq -r .display_name <<<"$output")" "Ops"
    assert_equal "$(jq -r .is_direct <<<"$output")" false
    assert_equal "$(jq -r .is_space <<<"$output")" false
    assert_regex "$(jq -r .matrix_to_uri <<<"$output")" "^https://matrix.to/#/"
    assert_equal "$(jq -r '[.members[].user_id] | sort | join(" ")' <<<"$output")" \
        "$(printf '%s\n' "$alice" "$bob" | sort | paste -sd ' ')"

    run mn_on b1 room info --room "$room"
    assert_line --regexp "^ *name +Ops * *\$"
    assert_line --regexp "^ *topic +alerts * *\$"
    assert_line --regexp "^ *encrypted +yes * *\$"
    assert_line --regexp "^ *MEMBER +DISPLAY NAME * *\$"
    assert_line --regexp "^ *$alice "
}

@test "info counts unread messages" {
    # Synapse's sliding sync reports no counts: they are always 0 there.
    [[ ${MN_SLIDING_SYNC:-} == 0 ]] || skip "Synapse's sliding sync has no unread counts"
    alice_and_bob_in_room
    mn_on a1 send --room "$room" "one"
    mn_on a1 send --room "$room" "two"

    run mn_on b1 --json room info --room "$room"
    assert_equal "$(jq -r .unread_notifications.notification_count <<<"$output")" 2
}

@test "info of a room the user is not in fails" {
    alice_and_bob_in_room
    other=$(mn_on a1 room create)

    run mn_on b1 room info --room "$other"
    assert_failure
}
