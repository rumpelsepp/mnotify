#!/usr/bin/env bats
# mn recovery reset

setup() {
    load helpers
    alice=$(new_user alice)
    login a1 "$alice"
}

@test "reset replaces the key: the old one stops working" {
    old=$(mn_on a1 recovery enable)

    run --separate-stderr mn_on a1 recovery reset
    assert_success
    assert_output --regexp '^([1-9A-HJ-NP-Za-km-z]{4} ){11}[1-9A-HJ-NP-Za-km-z]{4}$'
    new=$output
    refute [ "$old" = "$new" ]

    login a2 "$alice"
    run mn_on a2 recovery recover <<<"$old"
    assert_failure
    run mn_on a2 recovery recover <<<"$new"
    assert_success
}
