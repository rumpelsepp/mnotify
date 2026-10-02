#!/usr/bin/env bats
# Concurrent mn processes: one per profile and account at a time.

setup() {
    load helpers
}

teardown() {
    if [[ -n ${sync_pid:-} ]]; then
        stop "$sync_pid"
    fi
}

# Regression for c0efe5b: parallel encryption with one Olm session lost keys.
@test "parallel sends in an encrypted room all succeed and are decryptable" {
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
    assert_equal "$(jq '[.[] | select(.type == "m.room.message")] | length' <<<"$output")" 10
    assert_equal "$(jq '[.[] | select(.type == "m.room.encrypted")] | length' <<<"$output")" 0
}

@test "a waiting process says whom it waits for" {
    alice=$(new_user alice)
    login a1 "$alice"
    room=$(mn_on a1 room create --unencrypted)
    E2E_TIMEOUT=20 mn_on a1 sync --room "$room" >/dev/null 2>&1 &
    sync_pid=$!
    sleep 3

    E2E_TIMEOUT=5 run --separate-stderr mn_on a1 -vv whoami
    assert_equal "$status" 124 # still waiting when killed
    assert_regex "$stderr" "waiting for mn process [0-9]+"
}

@test "a second device of the same account sends while sync runs" {
    alice=$(new_user alice)
    login a1 "$alice"
    login_profile a1 sender "$alice"
    room=$(mn_on a1 room create --unencrypted)

    # sync holds the lock of the default profile for as long as it runs.
    E2E_TIMEOUT=40 mn_on a1 --json sync --room "$room" >"$BATS_TEST_TMPDIR/sync.jsonl" 2>/dev/null &
    sync_pid=$!

    # A send that waited for the sync to end would never get through here.
    got_live() {
        E2E_TIMEOUT=15 mn_on a1 -p sender send --room "$room" "live" >/dev/null || return 1
        jq -e 'select(.content.body == "live")' "$BATS_TEST_TMPDIR/sync.jsonl" >/dev/null
    }
    run wait_until 30 got_live
    assert_success
}
