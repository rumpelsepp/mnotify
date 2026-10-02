#!/usr/bin/env bats
# mn in scripts: clean stdout, JSON everywhere, exit codes, no prompts.

setup() {
    load helpers
}

@test "send prints nothing but the event ID on stdout" {
    alice_and_bob_in_room

    run --separate-stderr mn_on a1 send --room "$room" "hi"
    assert_success
    assert_output --regexp '^\$[^[:space:]]+$'
}

@test "--json output parses for every one-shot command" {
    alice_and_bob_in_room
    event=$(mn_on a1 send --room "$room" "hi")

    for cmd in "whoami" "homeserver" "room list" "room info --room $room" \
        "messages --room $room" "recovery status" "send --room $room json" \
        "room create" "room join $room"; do
        # shellcheck disable=SC2086 # word splitting is the point
        run --separate-stderr mn_on a1 --json $cmd
        assert_success
        jq -e . <<<"$output" >/dev/null || fail "not JSON: mn --json $cmd: $output"
    done
    assert [ -n "$event" ]
}

@test "commands that read stdin do not wait for a terminal" {
    alice_and_bob_in_room

    run mn_on a1 send --room "$room" </dev/null
    assert_failure
    assert_output --partial "empty message"

    run mn_on a1 recovery recover </dev/null
    assert_failure
    refute [ "$status" -eq 124 ] # 124 = killed by timeout
}

@test "parallel sends all succeed" {
    alice_and_bob_in_room

    pids=()
    for i in $(seq 10); do
        mn_on a1 send --room "$room" "parallel $i" >/dev/null &
        pids+=($!)
    done
    for pid in "${pids[@]}"; do
        wait "$pid" || fail "a parallel send failed"
    done

    run mn_on b1 --json messages --room "$room" --limit 30
    assert_equal "$(jq '[.[] | select(.type == "m.room.message" or .type == "m.room.encrypted")] | length' <<<"$output")" 10
}

@test "parallel sends in an encrypted room are all decryptable" {
    skip "known bug: concurrent sends from one device sometimes leave messages undecryptable"
    alice_and_bob_in_room

    pids=()
    for i in $(seq 10); do
        mn_on a1 send --room "$room" "parallel $i" >/dev/null &
        pids+=($!)
    done
    for pid in "${pids[@]}"; do
        wait "$pid"
    done

    run mn_on b1 --json messages --room "$room" --limit 30
    assert_equal "$(jq '[.[] | select(.type == "m.room.message")] | length' <<<"$output")" 10
}

@test "errors exit non-zero" {
    alice_and_bob_in_room

    run mn_on a1 send --room "#does-not-exist:localhost" "x"
    assert_failure

    other=$(mn_on a1 room create)
    run mn_on b1 send --room "$other" "x"
    assert_failure
    assert_output --partial "not a member"

    run mn_on nobody whoami
    assert_failure
}
