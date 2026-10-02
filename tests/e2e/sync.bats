#!/usr/bin/env bats
# mn sync

setup() {
    load helpers
}

teardown() {
    if [[ -n ${sync_pid:-} ]]; then
        stop "$sync_pid"
    fi
}

# start_sync DEVICE ARGS...: run `mn sync ARGS` on DEVICE in the background,
# its stdout into $BATS_TEST_TMPDIR/sync.out.
start_sync() {
    local device=$1
    shift
    E2E_TIMEOUT=60 mn_on "$device" sync "$@" >"$BATS_TEST_TMPDIR/sync.out" 2>/dev/null &
    sync_pid=$!
}

# send_until_seen DEVICE ROOM TEXT: sync only prints what arrives after it
# started, so send TEXT until sync printed it.
send_until_seen() {
    local device=$1 room=$2 text=$3
    sent_and_seen() {
        mn_on "$device" send --room "$room" "$text" >/dev/null
        grep -qF "$text" "$BATS_TEST_TMPDIR/sync.out"
    }
    wait_until 30 sent_and_seen
}

@test "--json prints new events as JSON lines" {
    alice_and_bob_in_room --unencrypted
    start_sync b1 --json --room "$room"

    run send_until_seen a1 "$room" "live"
    assert_success
    run jq -e -s 'map(select(.content.body == "live")) | length > 0' "$BATS_TEST_TMPDIR/sync.out"
    assert_success
}

@test "messages in an encrypted room are printed decrypted" {
    [[ ${MN_SLIDING_SYNC:-} == 0 ]] || skip "known bug: with sliding sync a message whose room key arrives later is printed undecrypted"
    alice_and_bob_in_room
    start_sync b1 --json --room "$room"

    run send_until_seen a1 "$room" "secret"
    assert_success
    run jq -c 'select(.type == "m.room.encrypted")' "$BATS_TEST_TMPDIR/sync.out"
    assert_output ""
}

@test "older messages are not printed again" {
    [[ ${MN_SLIDING_SYNC:-} == 0 ]] || skip "known bug: with sliding sync the first mn sync prints the room's history"
    alice_and_bob_in_room
    mn_on a1 send --room "$room" "old" >/dev/null
    mn_on b1 messages --room "$room" >/dev/null
    start_sync b1 --json --room "$room"

    run send_until_seen a1 "$room" "new"
    assert_success
    run grep -F '"old"' "$BATS_TEST_TMPDIR/sync.out"
    assert_failure
}

@test "without --json one line per event" {
    alice_and_bob_in_room
    start_sync b1 --room "$room"

    run send_until_seen a1 "$room" "live"
    assert_success
    run grep -E "^[0-9-]{10} [0-9:]{8}  $alice  live\$" "$BATS_TEST_TMPDIR/sync.out"
    assert_success
}

@test "--room only prints that room, without --room all rooms" {
    alice_and_bob_in_room
    other=$(mn_on a1 room create --invite "$bob")
    mn_on b1 room join "$other" >/dev/null

    start_sync b1 --json --room "$room"
    run send_until_seen a1 "$room" "in room"
    assert_success
    mn_on a1 send --room "$other" "in other" >/dev/null
    run send_until_seen a1 "$room" "in room again"
    assert_success
    run grep -F "in other" "$BATS_TEST_TMPDIR/sync.out"
    assert_failure
    stop "$sync_pid"

    start_sync b1 --json
    run send_until_seen a1 "$room" "everywhere 1"
    assert_success
    run send_until_seen a1 "$other" "everywhere 2"
    assert_success
}

@test "--receipt marks what it printed as read" {
    alice_and_bob_in_room
    bob_token=$(token_of b1)

    start_sync b1 --json --room "$room" --receipt
    run send_until_seen a1 "$room" "read me"
    assert_success

    # Bob's read receipt on the last message, as the server reports it.
    receipt_of_bob() {
        local event
        event=$(jq -r 'select(.content.body == "read me") | .event_id' "$BATS_TEST_TMPDIR/sync.out" | tail -n 1)
        client_sync "$bob_token" | jq -e --arg room "$room" --arg event "$event" --arg bob "$bob" \
            '.rooms.join[$room].ephemeral.events[] | select(.type == "m.receipt") | .content[$event]["m.read"][$bob]' >/dev/null
    }
    run wait_until 10 receipt_of_bob
    assert_success
}

@test "without --receipt nothing is marked as read" {
    alice_and_bob_in_room
    bob_token=$(token_of b1)

    start_sync b1 --json --room "$room"
    run send_until_seen a1 "$room" "unread"
    assert_success

    run client_sync "$bob_token"
    assert_equal "$(jq --arg room "$room" --arg bob "$bob" \
        '[.rooms.join[$room].ephemeral.events[] | select(.type == "m.receipt") | .content[][]?[$bob]? | select(. != null)] | length' <<<"$output")" 0
}

@test "an unknown room fails right away" {
    alice=$(new_user alice)
    login a1 "$alice"

    run mn_on a1 sync --room "#does-not-exist:localhost"
    assert_failure
}
