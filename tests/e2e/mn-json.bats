#!/usr/bin/env bats
# mn --json: JSON on stdout for every command that prints something.

setup() {
    load helpers
}

@test "--json output parses for every one-shot command" {
    alice_and_bob_in_room
    other=$(mn_on a1 room create --public)
    event=$(mn_on a1 send --room "$room" "root")

    for cmd in "whoami" "homeserver" "room list" "room info --room $room" \
        "messages --room $room" "messages --room $room --thread $event" \
        "recovery status" "send --room $room json" \
        "room create" "room join $other" "recovery enable" "recovery reset"; do
        # shellcheck disable=SC2086 # word splitting is the point
        run --separate-stderr mn_on a1 --json $cmd
        assert_success || fail "mn --json $cmd failed: $stderr"
        jq -e . <<<"$output" >/dev/null || fail "not JSON: mn --json $cmd: $output"
    done
}

@test "commands without output print nothing, also with --json" {
    alice_and_bob_in_room
    event=$(mn_on a1 send --room "$room" "x")
    mn_on a1 recovery enable >/dev/null

    for cmd in "typing --room $room" "redact --room $room --event-id $event" \
        "recovery disable"; do
        # shellcheck disable=SC2086 # word splitting is the point
        run --separate-stderr mn_on a1 --json $cmd
        assert_success || fail "mn --json $cmd failed: $stderr"
        assert_output ""
    done
}
