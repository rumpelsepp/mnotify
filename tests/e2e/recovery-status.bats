#!/usr/bin/env bats
# mn recovery status

setup() {
    load helpers
}

@test "status of a fresh account: cross-signed, no recovery, no backup" {
    alice=$(new_user alice)
    login a1 "$alice"

    run --separate-stderr mn_on a1 --json recovery status
    assert_success
    assert_equal "$(jq -r .recovery <<<"$output")" Disabled
    assert_equal "$(jq -r .backup_on_server <<<"$output")" false
    assert_equal "$(jq -r .cross_signing_complete <<<"$output")" true
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" true

    run mn_on a1 recovery status
    assert_line --regexp "^ *recovery +Disabled *\$"
    assert_line --regexp "^ *device cross signed +yes *\$"
}

@test "another device sees the recovery the first one enabled" {
    alice=$(new_user alice)
    login a1 "$alice"
    mn_on a1 recovery enable >/dev/null
    login a2 "$alice"

    run mn_on a2 --json recovery status
    assert_equal "$(jq -r .backup_on_server <<<"$output")" true
    assert_equal "$(jq -r .device_cross_signed <<<"$output")" false
}
