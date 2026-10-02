#!/usr/bin/env bats
# mn recovery enable

setup() {
    load helpers
    alice=$(new_user alice)
    login a1 "$alice"
}

@test "enable prints a recovery key and turns on the backup" {
    run --separate-stderr mn_on a1 recovery enable
    assert_success
    assert_output --regexp '^([1-9A-HJ-NP-Za-km-z]{4} ){11}[1-9A-HJ-NP-Za-km-z]{4}$'
    key=$output

    run mn_on a1 --json recovery status
    assert_equal "$(jq -r .recovery <<<"$output")" Enabled
    assert_equal "$(jq -r .backup <<<"$output")" Enabled
    assert_equal "$(jq -r .backup_on_server <<<"$output")" true

    # The key is the one that works.
    login a2 "$alice"
    run mn_on a2 recovery recover <<<"$key"
    assert_success
}

@test "--json prints the key as recovery_key" {
    run --separate-stderr mn_on a1 --json recovery enable
    assert_success
    assert_regex "$(jq -r .recovery_key <<<"$output")" '^([1-9A-HJ-NP-Za-km-z]{4} ){11}'
}

@test "enable twice is refused" {
    mn_on a1 recovery enable >/dev/null

    run mn_on a1 recovery enable
    assert_failure
    assert_output --partial "already enabled"
}
