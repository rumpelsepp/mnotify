#!/usr/bin/env bats
# mn whoami

setup() {
    load helpers
}

@test "whoami names the user and this device" {
    alice=$(new_user alice)
    login a1 "$alice"
    login a2 "$alice"

    run mn_on a1 --json whoami
    assert_success
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
    assert_equal "$(jq -r .is_guest <<<"$output")" false
    a1_device=$(jq -r .device_id <<<"$output")
    assert [ -n "$a1_device" ]

    # Same account, another device.
    refute [ "$(mn_on a2 --json whoami | jq -r .device_id)" = "$a1_device" ]

    run mn_on a1 whoami
    assert_success
    assert_line --regexp "^ *user id +$alice *\$"
    assert_line --regexp "^ *device id +$a1_device *\$"
    assert_line --regexp "^ *is guest +no *\$"
}

@test "whoami without a login fails" {
    run mn_on a1 whoami
    assert_failure
    assert_output --partial "not logged in"
}
