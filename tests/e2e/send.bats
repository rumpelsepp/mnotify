#!/usr/bin/env bats
# mn send

setup() {
    load helpers
}

@test "text in an unencrypted room; stdout is just the event ID" {
    alice_and_bob_in_room --unencrypted

    run --separate-stderr mn_on a1 send --room "$room" "hello"
    assert_success
    assert_output --regexp '^\$[^[:space:]]+$'
    event=$output

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_body)" "hello"
    assert_equal "$(jq -r '[.[] | select(.type == "m.room.message")] | last | .event_id' <<<"$output")" "$event"
    assert_equal "$(last_content | jq -r .msgtype)" "m.text"
}

@test "text in an encrypted room" {
    alice_and_bob_in_room

    run mn_on a1 send --room "$room" "secret"
    assert_success

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_body)" "secret"
    refute_output --partial "m.room.encrypted"
}

@test "--json prints the room and event ID" {
    alice_and_bob_in_room

    run --separate-stderr mn_on a1 --json send --room "$room" "hi"
    assert_success
    assert_equal "$(jq -r .room_id <<<"$output")" "$room"
    assert_regex "$(jq -r .event_id <<<"$output")" '^\$'
}

@test "multi-line text from stdin keeps its lines, minus the final newline" {
    alice_and_bob_in_room

    printf 'line 1\nline 2\n' | mn_on a1 send --room "$room"

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_body)" $'line 1\nline 2'
}

@test "an empty message is refused, also without stdin" {
    alice_and_bob_in_room

    run mn_on a1 send --room "$room" <<<""
    assert_failure
    assert_output --partial "empty message"

    run mn_on a1 send --room "$room" "   "
    assert_failure
    assert_output --partial "empty message"

    # No terminal and nothing on stdin: fail right away, never wait.
    run mn_on a1 send --room "$room" </dev/null
    assert_failure
    assert_output --partial "empty message"
}

@test "markdown keeps a plain-text body" {
    alice_and_bob_in_room

    mn_on a1 send --room "$room" --markdown "**bold** and \`code\`"

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_content | jq -r .format)" "org.matrix.custom.html"
    assert_equal "$(last_content | jq -r .formatted_body)" "<strong>bold</strong> and <code>code</code>"
    assert_equal "$(last_body)" "**bold** and \`code\`"
}

@test "notice and emote, also as markdown" {
    alice_and_bob_in_room

    mn_on a1 send --room "$room" --notice "bot speaking"
    mn_on a1 send --room "$room" --emote "waves"
    mn_on a1 send --room "$room" --notice --markdown "**done**"

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(contents | jq -r '.[-3].msgtype')" "m.notice"
    assert_equal "$(contents | jq -r '.[-2].msgtype')" "m.emote"
    assert_equal "$(contents | jq -r '.[-2].body')" "waves"
    assert_equal "$(contents | jq -r '.[-1].msgtype')" "m.notice"
    assert_equal "$(contents | jq -r '.[-1].formatted_body')" "<strong>done</strong>"
}

@test "conflicting kinds are refused" {
    alice_and_bob_in_room

    run mn_on a1 send --room "$room" --notice --emote "x"
    assert_failure
    run mn_on a1 send --room "$room" --attachment /etc/hostname "x"
    assert_failure
    run mn_on a1 send --room "$room" --reply-to '$a' --thread '$b' "x"
    assert_failure
}

@test "a file attachment" {
    alice_and_bob_in_room
    echo "log line" >"$BATS_TEST_TMPDIR/backup.log"

    run mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/backup.log"
    assert_success

    run mn_on b1 --json messages --room "$room"
    content=$(last_content)
    assert_equal "$(jq -r .msgtype <<<"$content")" "m.file"
    assert_equal "$(jq -r .body <<<"$content")" "backup.log"
    assert_equal "$(jq -r .info.size <<<"$content")" 9
    # Encrypted room: the file is encrypted too and its key travels in the event.
    assert_equal "$(jq -r '.file.key.alg' <<<"$content")" "A256CTR"
    assert_equal "$(jq -r '.url' <<<"$content")" null
}

@test "a file attachment in an unencrypted room is a plain upload" {
    alice_and_bob_in_room --unencrypted
    echo "log line" >"$BATS_TEST_TMPDIR/backup.log"

    mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/backup.log"

    run mn_on b1 --json messages --room "$room"
    assert_regex "$(last_content | jq -r .url)" '^mxc://'
    assert_equal "$(last_content | jq -r .file)" null
}

@test "a missing file fails" {
    alice_and_bob_in_room

    run mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/nope"
    assert_failure
}

@test "an image gets its size, above 800px a thumbnail" {
    alice_and_bob_in_room
    python3 "$BATS_TEST_DIRNAME/make_png.py" 1200 300 "$BATS_TEST_TMPDIR/wide.png"
    python3 "$BATS_TEST_DIRNAME/make_png.py" 100 50 "$BATS_TEST_TMPDIR/small.png"

    mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/wide.png"
    mn_on a1 send --room "$room" --attachment "$BATS_TEST_TMPDIR/small.png"

    run mn_on b1 --json messages --room "$room"
    wide=$(contents | jq -c '.[-2]')
    assert_equal "$(jq -r .msgtype <<<"$wide")" "m.image"
    assert_equal "$(jq -r .info.w <<<"$wide")" 1200
    assert_equal "$(jq -r .info.h <<<"$wide")" 300
    assert_equal "$(jq -r .info.thumbnail_info.w <<<"$wide")" 800
    assert_equal "$(jq -r .info.thumbnail_info.h <<<"$wide")" 200
    small=$(contents | jq -c '.[-1]')
    assert_equal "$(jq -r .msgtype <<<"$small")" "m.image"
    assert_equal "$(jq -r .info.w <<<"$small")" 100
    assert_equal "$(jq -r .info.thumbnail_info <<<"$small")" null
}

@test "--reply-to answers one message" {
    alice_and_bob_in_room
    root=$(mn_on a1 send --room "$room" "deploy started")

    mn_on b1 send --room "$room" --reply-to "$root" "ack"

    run mn_on a1 --json messages --room "$room"
    assert_equal "$(last_content | jq -r '.["m.relates_to"]["m.in_reply_to"].event_id')" "$root"
    assert_equal "$(last_content | jq -r '.["m.relates_to"].rel_type')" null
}

@test "--thread starts a thread and continues it from any of its events" {
    alice_and_bob_in_room
    root=$(mn_on a1 send --room "$room" "deploy started")

    first=$(mn_on a1 send --room "$room" --thread "$root" "step 1")
    mn_on b1 send --room "$room" --thread "$first" --notice "step 2"

    run mn_on b1 --json messages --room "$room"
    for i in -2 -1; do
        assert_equal "$(contents | jq -r ".[$i][\"m.relates_to\"].rel_type")" "m.thread"
        assert_equal "$(contents | jq -r ".[$i][\"m.relates_to\"].event_id")" "$root"
    done
}

@test "a file goes into a thread too" {
    alice_and_bob_in_room
    root=$(mn_on a1 send --room "$room" "deploy started")
    echo "log line" >"$BATS_TEST_TMPDIR/deploy.log"

    mn_on a1 send --room "$room" --thread "$root" --attachment "$BATS_TEST_TMPDIR/deploy.log"

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_content | jq -r .msgtype)" "m.file"
    assert_equal "$(last_content | jq -r '.["m.relates_to"].rel_type')" "m.thread"
    assert_equal "$(last_content | jq -r '.["m.relates_to"].event_id')" "$root"
}

@test "a reply to a message in a thread stays in the thread" {
    alice_and_bob_in_room
    root=$(mn_on a1 send --room "$room" "deploy started")
    step=$(mn_on a1 send --room "$room" --thread "$root" "step 1")

    mn_on b1 send --room "$room" --reply-to "$step" "why?"

    run mn_on a1 --json messages --room "$room"
    relation=$(last_content | jq -c '.["m.relates_to"]')
    assert_equal "$(jq -r .rel_type <<<"$relation")" "m.thread"
    assert_equal "$(jq -r .event_id <<<"$relation")" "$root"
    assert_equal "$(jq -r '.["m.in_reply_to"].event_id' <<<"$relation")" "$step"
}

@test "mentions are declared, names in the text are not" {
    alice=$(new_user alice)
    bob=$(new_user bob)
    carol=$(new_user carol)
    login a1 "$alice"
    login b1 "$bob"
    room=$(mn_on a1 room create --invite "$bob")
    mn_on b1 room join "$room" >/dev/null

    mn_on a1 send --room "$room" --mention "$bob" --mention "$carol" "disk full"
    mn_on a1 send --room "$room" "$bob is not pinged"

    run mn_on b1 --json messages --room "$room"
    assert_equal "$(contents | jq -r '.[-2]["m.mentions"].user_ids | sort | join(" ")')" \
        "$(printf '%s\n' "$bob" "$carol" | sort | paste -sd ' ')"
    assert_equal "$(contents | jq -c '.[-1]["m.mentions"]')" "{}"
}

@test "@room needs the power level for it" {
    alice_and_bob_in_room

    run mn_on b1 send --room "$room" --mention-room "everyone!"
    assert_failure

    # Alice created the room and has the power level.
    run mn_on a1 send --room "$room" --mention-room "everyone!"
    assert_success
    run mn_on b1 --json messages --room "$room"
    assert_equal "$(last_content | jq -r '.["m.mentions"].room')" true
}

@test "the room can be an alias or MN_ROOM" {
    alice=$(new_user alice)
    login a1 "$alice"
    alias=${alice#@}
    alias=${alias%%:*}
    room=$(mn_on a1 room create --alias "$alias")

    run mn_on a1 send --room "#$alias:localhost" "via alias"
    assert_success
    run mn_on a1 MN_ROOM="$room" send "via env"
    assert_success
    # --room wins over MN_ROOM.
    run mn_on a1 MN_ROOM="!nope:localhost" send --room "$room" "via option"
    assert_success

    run mn_on a1 --json messages --room "$room"
    assert_equal "$(contents | jq -r '[.[-3:][].body] | join(",")')" "via alias,via env,via option"
}

@test "a room that is unknown, not joined or only invited fails" {
    alice_and_bob_in_room

    run mn_on a1 send --room "#does-not-exist:localhost" "x"
    assert_failure

    other=$(mn_on a1 room create)
    run mn_on b1 send --room "$other" "x"
    assert_failure
    assert_output --partial "not a member"

    invited=$(mn_on a1 room create --invite "$bob")
    run mn_on b1 send --room "$invited" "x"
    assert_failure
    assert_output --partial "pending invite"
}

@test "a device that is not cross-signed cannot send encrypted, but plain" {
    alice_and_bob_in_room
    plain=$(mn_on a1 room create --unencrypted --invite "$bob")
    login a2 "$alice"

    run mn_on a2 send --room "$room" "from a2"
    assert_failure
    assert_output --partial "not cross-signed"

    run mn_on a2 send --room "$plain" "plain"
    assert_success
}
