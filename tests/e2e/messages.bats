#!/usr/bin/env bats
# mn messages

setup() {
    load helpers
}

@test "the latest messages, oldest first, at most --limit events" {
    alice_and_bob_in_room
    for i in 1 2 3 4 5; do
        mn_on a1 send --room "$room" "message $i" >/dev/null
    done

    run --separate-stderr mn_on b1 --json messages --room "$room" --limit 3
    assert_success
    assert_equal "$(jq length <<<"$output")" 3
    assert_equal "$(jq -r '[.[].content.body] | join(",")' <<<"$output")" "message 3,message 4,message 5"

    # The default is 10.
    run --separate-stderr mn_on b1 --json messages --room "$room"
    assert_equal "$(jq length <<<"$output")" 10
}

@test "the human form is one line per event" {
    alice_and_bob_in_room
    root=$(mn_on a1 send --room "$room" "plain")
    mn_on a1 send --room "$room" --emote "waves"
    echo x >"$BATS_TEST_TMPDIR/a.log"
    mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/a.log"
    mn_on a1 send --room "$room" --thread "$root" "in thread"
    printf 'two\nlines' | mn_on a1 send --room "$room"
    gone=$(mn_on a1 send --room "$room" "oops")
    mn_on a1 redact --room "$room" --event-id "$gone"

    run --separate-stderr mn_on b1 messages --room "$room" --limit 7
    assert_success
    ts='[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}'
    assert_line --index 0 --regexp "^$ts  $alice  plain\$"
    assert_line --index 1 --regexp "^$ts  $alice  \\* waves\$"
    assert_line --index 2 --regexp "^$ts  $alice  \\[file\\] a.log\$"
    assert_line --index 3 --regexp "^$ts  $alice  ↳ in thread\$"
    assert_line --index 4 --regexp "^$ts  $alice  two\$"
    assert_line --index 5 --regexp "^ +lines\$"
    assert_line --index 6 --regexp "^$ts  $alice  \\[deleted\\]\$"
}

@test "--thread shows the root and its replies only" {
    alice_and_bob_in_room
    root=$(mn_on a1 send --room "$room" "deploy started")
    mn_on a1 send --room "$room" --thread "$root" "step 1"
    mn_on a1 send --room "$room" "unrelated"
    mn_on b1 send --room "$room" --thread "$root" "step 2"

    run --separate-stderr mn_on b1 --json messages --room "$room" --thread "$root"
    assert_success
    assert_equal "$(jq -r '[.[].content.body] | join(",")' <<<"$output")" "deploy started,step 1,step 2"

    run --separate-stderr mn_on b1 --json messages --room "$room" --thread "$root" --limit 1
    assert_equal "$(jq -r '[.[].content.body] | join(",")' <<<"$output")" "deploy started,step 2"
}

@test "events of other kinds come along" {
    alice_and_bob_in_room

    run mn_on b1 --json messages --room "$room" --limit 50
    assert_equal "$(jq -r '[.[] | select(.type == "m.room.member") | .content.membership + " " + .state_key] | join(",")' <<<"$output")" \
        "join $alice,invite $bob,join $bob"
    assert_equal "$(jq '[.[] | select(.type == "m.room.encryption")] | length' <<<"$output")" 1
}

@test "a room the user is not in fails" {
    alice_and_bob_in_room
    other=$(mn_on a1 room create)

    run mn_on b1 messages --room "$other"
    assert_failure
    assert_output --partial "not a member"
}
