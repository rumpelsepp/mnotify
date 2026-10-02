#!/usr/bin/env bats
# mn recovery disable

setup() {
    load helpers
    alice=$(new_user alice)
    login a1 "$alice"
}

@test "disable deletes the backup and the key stops working" {
    key=$(mn_on a1 recovery enable)

    run --separate-stderr mn_on a1 recovery disable
    assert_success
    assert_output ""

    run mn_on a1 --json recovery status
    assert_equal "$(jq -r .recovery <<<"$output")" Disabled
    assert_equal "$(jq -r .backup_on_server <<<"$output")" false

    login a2 "$alice"
    run mn_on a2 recovery recover <<<"$key"
    assert_failure
}

@test "enable works again after disable" {
    mn_on a1 recovery enable >/dev/null
    mn_on a1 recovery disable

    run mn_on a1 recovery enable
    assert_success
}
