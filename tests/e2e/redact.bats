#!/usr/bin/env bats
# mn redact

setup() {
    load helpers
}

@test "redact removes the content and keeps the reason" {
    alice_and_bob_in_room
    event=$(mn_on a1 send --room "$room" "oops")

    run --separate-stderr mn_on a1 redact --room "$room" --event-id "$event" --reason "typo"
    assert_success
    assert_output ""

    run mn_on b1 --json messages --room "$room"
    redacted=$(jq -c --arg id "$event" '.[] | select(.event_id == $id)' <<<"$output")
    assert_equal "$(jq -c .content <<<"$redacted")" "{}"
    assert_equal "$(jq -r .unsigned.redacted_because.content.reason <<<"$redacted")" "typo"
}

@test "a file can be redacted" {
    alice_and_bob_in_room
    echo x >"$BATS_TEST_TMPDIR/a.log"
    event=$(mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/a.log")

    run mn_on a1 redact --room "$room" --event-id "$event"
    assert_success

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(jq -c --arg id "$event" '.[] | select(.event_id == $id) | .content' <<<"$output")" "{}"
}

@test "others' messages need the power level" {
    alice_and_bob_in_room
    event=$(mn_on a1 send --room "$room" "mine")

    run mn_on b1 redact --room "$room" --event-id "$event"
    assert_failure

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_body)" "mine"

    # Alice created the room and may redact Bob's messages.
    event=$(mn_on b1 send --room "$room" "bob's")
    run mn_on a1 redact --room "$room" --event-id "$event"
    assert_success
}

