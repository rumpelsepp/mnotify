# Shared setup for the e2e suite: loaded by every .bats file.
#
# Every test gets fresh users, and every "device" is its own XDG_STATE_HOME
# below $BATS_TEST_TMPDIR, so tests share the homeserver but no state.

bats_require_minimum_version 1.5.0
bats_load_library bats-support
bats_load_library bats-assert

E2E_HOMESERVER=${E2E_HOMESERVER:-http://localhost:8008}
E2E_TIMEOUT=${E2E_TIMEOUT:-60}
MN=${MN:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/target/release/mn}

# Create a fresh user with synadm (as the suite's admin, see setup_suite)
# and print its ID. The password is derived from the localpart, see
# `password_of`.
new_user() {
    local localpart="$1-${BATS_SUITE_TEST_NUMBER:-0}-$RANDOM$RANDOM"
    local user_id="@$localpart:localhost"
    synadm user modify "$user_id" --password "$(password_of "$user_id")" >/dev/null
    echo "$user_id"
}

synadm() {
    $E2E_SYNADM --config-file "$E2E_SYNADM_CONFIG" --batch "$@"
}

password_of() {
    local localpart=${1#@}
    echo "pw-${localpart%%:*}"
}

# mn_on DEVICE [VAR=value...] ARGS...: run the real mn as DEVICE, in a clean
# environment plus the given variables.
mn_on() {
    local device=$1
    shift
    local vars=()
    while [[ ${1:-} =~ ^[A-Z_]+= ]]; do
        vars+=("$1")
        shift
    done
    env -u MN_ROOM -u MN_PROFILE -u RUST_BACKTRACE -u RUST_LOG \
        XDG_STATE_HOME="$BATS_TEST_TMPDIR/$device" MN_NO_KEYRING=1 "${vars[@]}" \
        timeout "$E2E_TIMEOUT" "$MN" "$@"
}

# login DEVICE USER: password login of USER as a new device named DEVICE.
login() {
    mn_on "$1" login "$2" --homeserver "$E2E_HOMESERVER" --device-name "$1" \
        <<<"$(password_of "$2")" 2>/dev/null
}

# The text of the last m.room.message in $output (JSON of `mn messages`).
last_body() {
    jq -r '[.[] | select(.type == "m.room.message")] | last | .content.body' <<<"$output"
}

# wait_until SECONDS COMMAND...: retry COMMAND until it succeeds.
wait_until() {
    local deadline=$((SECONDS + $1))
    shift
    until "$@"; do
        ((SECONDS < deadline)) || return 1
        sleep 0.5
    done
}

# Alice (device a1) and Bob (device b1), both joined to a fresh room $room.
# Extra arguments go to `mn room create`.
alice_and_bob_in_room() {
    alice=$(new_user alice)
    bob=$(new_user bob)
    login a1 "$alice"
    login b1 "$bob"
    room=$(mn_on a1 room create --invite "$bob" "$@")
    mn_on b1 room join "$room" >/dev/null
}
