#!/usr/bin/env bats
# Sending and receiving: text, files, relations, mentions, redactions, sync.

setup() {
    load helpers
}

@test "text in an unencrypted room" {
    alice_and_bob_in_room --unencrypted

    run mn_on a1 send --room "$room" "hello"
    assert_success
    assert_output --regexp '^\$[^[:space:]]+$'

    run mn_on b1 --json messages --room "$room"
    assert_success
    assert_equal "$(last_body)" "hello"
}

@test "text in an encrypted room" {
    alice_and_bob_in_room

    run mn_on a1 send --room "$room" "secret"
    assert_success

    run mn_on b1 --json messages --room "$room"
    assert_success
    assert_equal "$(last_body)" "secret"
    refute_output --partial "m.room.encrypted"
}

@test "multi-line text from stdin keeps its lines, minus the final newline" {
    alice_and_bob_in_room

    printf 'line 1\nline 2\n' | mn_on a1 send --room "$room"

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_body)" $'line 1\nline 2'
}

@test "an empty message is refused" {
    alice_and_bob_in_room

    run mn_on a1 send --room "$room" <<<""
    assert_failure
    assert_output --partial "empty message"
}

@test "markdown, notice and emote" {
    alice_and_bob_in_room

    mn_on a1 send --room "$room" --markdown "**bold**"
    mn_on a1 send --room "$room" --notice "bot speaking"
    mn_on a1 send --room "$room" --emote "waves"

    run mn_on b1 --json messages --room "$room"
    messages=$(jq -c '[.[] | select(.type == "m.room.message") | .content]' <<<"$output")
    assert_equal "$(jq -r '.[-3].formatted_body' <<<"$messages")" "<strong>bold</strong>"
    assert_equal "$(jq -r '.[-2].msgtype' <<<"$messages")" "m.notice"
    assert_equal "$(jq -r '.[-1].msgtype' <<<"$messages")" "m.emote"
}

@test "a file attachment" {
    alice_and_bob_in_room
    echo "log line" >"$BATS_TEST_TMPDIR/backup.log"

    run mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/backup.log"
    assert_success

    run mn_on b1 --json messages --room "$room"
    content=$(jq -c '[.[] | select(.type == "m.room.message")] | last | .content' <<<"$output")
    assert_equal "$(jq -r .msgtype <<<"$content")" "m.file"
    assert_equal "$(jq -r .body <<<"$content")" "backup.log"
    # Encrypted room: the file is encrypted too and its key travels in the event.
    assert_equal "$(jq -r '.file.key.alg' <<<"$content")" "A256CTR"
}

@test "a large image gets its size and a thumbnail" {
    alice_and_bob_in_room
    python3 "$BATS_TEST_DIRNAME/make_png.py" 1200 300 "$BATS_TEST_TMPDIR/wide.png"

    mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/wide.png"

    run mn_on b1 --json messages --room "$room"
    content=$(jq -c '[.[] | select(.type == "m.room.message")] | last | .content' <<<"$output")
    assert_equal "$(jq -r .msgtype <<<"$content")" "m.image"
    assert_equal "$(jq -r .info.w <<<"$content")" 1200
    assert_equal "$(jq -r .info.h <<<"$content")" 300
    assert_equal "$(jq -r .info.thumbnail_info.w <<<"$content")" 800
}

@test "reply and thread" {
    alice_and_bob_in_room
    root=$(mn_on a1 send --room "$room" "deploy started")

    mn_on b1 send --room "$room" --reply-to "$root" "ack"
    mn_on a1 send --room "$room" --thread "$root" "step 1 done"

    run mn_on b1 --json messages --room "$room"
    relations=$(jq -c '[.[] | select(.type == "m.room.message") | .content["m.relates_to"]]' <<<"$output")
    assert_equal "$(jq -r '.[-2]["m.in_reply_to"].event_id' <<<"$relations")" "$root"
    assert_equal "$(jq -r '.[-1].rel_type' <<<"$relations")" "m.thread"
    assert_equal "$(jq -r '.[-1].event_id' <<<"$relations")" "$root"

    run mn_on b1 --json messages --room "$room" --thread "$root"
    assert_success
    assert_equal "$(last_body)" "step 1 done"
}

@test "mentions are declared, @room needs the power level" {
    alice_and_bob_in_room

    mn_on a1 send --room "$room" --mention "$bob" "disk full"
    run mn_on b1 --json messages --room "$room"
    assert_equal "$(jq -r '[.[] | select(.type == "m.room.message")] | last | .content["m.mentions"].user_ids[0]' <<<"$output")" "$bob"

    run mn_on b1 send --room "$room" --mention-room "everyone!"
    assert_failure
}

@test "redact removes the content" {
    alice_and_bob_in_room
    event=$(mn_on a1 send --room "$room" "oops")

    run mn_on a1 redact --room "$room" --event-id "$event" --reason "typo"
    assert_success

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(jq -r --arg id "$event" '.[] | select(.event_id == $id) | .content.body // "gone"' <<<"$output")" "gone"
}

@test "sync prints new messages as JSON lines" {
    alice_and_bob_in_room
    E2E_TIMEOUT=30 mn_on b1 --json sync --room "$room" >"$BATS_TEST_TMPDIR/sync.jsonl" 2>/dev/null &
    sync_pid=$!

    # sync only prints what arrives after it started; send until one shows up.
    got_live() {
        mn_on a1 send --room "$room" "live" >/dev/null
        jq -e 'select(.content.body == "live")' "$BATS_TEST_TMPDIR/sync.jsonl" >/dev/null
    }
    run wait_until 20 got_live
    kill "$sync_pid" 2>/dev/null || true
    assert_success
}

@test "MN_ROOM replaces --room" {
    alice_and_bob_in_room

    run mn_on a1 MN_ROOM="$room" send "via env"
    assert_success

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_body)" "via env"
}
