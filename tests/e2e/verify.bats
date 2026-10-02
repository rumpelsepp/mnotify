#!/usr/bin/env bats
# mn verify: emoji verification between two devices of one account.

setup() {
    load helpers
    alice=$(new_user alice)
    login a1 "$alice"
    login a2 "$alice"
    a1_device=$(mn_on a1 --json whoami | jq -r .device_id)
}

teardown() {
    if [[ -n ${waiting_pid:-} ]]; then
        stop "$waiting_pid"
    fi
}

# verify_on DEVICE ANSWER ARGS...: `mn verify ARGS` as DEVICE in a terminal,
# as the emoji prompt needs one, answering it with ANSWER; the output goes
# to $BATS_TEST_TMPDIR/DEVICE.out.
verify_on() {
    local device=$1 answer=$2
    shift 2
    env -u MN_ROOM -u MN_PROFILE -u RUST_BACKTRACE -u RUST_LOG \
        XDG_STATE_HOME="$BATS_TEST_TMPDIR/$device" MN_NO_KEYRING=1 \
        timeout "$E2E_TIMEOUT" python3 "$BATS_TEST_DIRNAME/pty_answer.py" "$answer" \
        "$BATS_TEST_TMPDIR/$device.out" "$MN" verify "$@"
}

# The emoji line a device was shown.
emojis_of() {
    grep -A1 "Confirm that the emojis match" "$BATS_TEST_TMPDIR/$1.out" | tail -n 1 | tr -d '\r'
}

@test "a new device is cross-signed after confirming the emojis on both" {
    verify_on a1 y &
    waiting_pid=$!
    wait_until 20 grep -qs "Waiting for a verification request" "$BATS_TEST_TMPDIR/a1.out"

    run verify_on a2 y --device "$a1_device"
    assert_success
    run grep -q "successfully verified device $alice $a1_device" "$BATS_TEST_TMPDIR/a2.out"
    assert_success
    run wait_until 10 grep -q "successfully verified device $alice" "$BATS_TEST_TMPDIR/a1.out"
    assert_success
    assert [ -n "$(emojis_of a1)" ]
    assert_equal "$(emojis_of a1)" "$(emojis_of a2)"

    stop "$waiting_pid"
    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" true
}

@test "declining the emojis cancels on both sides" {
    verify_on a1 y &
    waiting_pid=$!
    wait_until 20 grep -qs "Waiting for a verification request" "$BATS_TEST_TMPDIR/a1.out"

    run verify_on a2 n --device "$a1_device"
    assert_failure
    run grep -q "verification cancelled" "$BATS_TEST_TMPDIR/a2.out"
    assert_success
    run wait_until 10 grep -q "verification cancelled" "$BATS_TEST_TMPDIR/a1.out"
    assert_success

    stop "$waiting_pid"
    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" false
}

@test "an unknown device fails right away" {
    run mn_on a2 verify --device NOSUCHDEVICE
    assert_failure
    assert_output --partial "no such device"
}
