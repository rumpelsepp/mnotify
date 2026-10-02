#!/usr/bin/env bats
# mn homeserver (alias hs)

setup() {
    load helpers
    alice=$(new_user alice)
    login a1 "$alice"
}

@test "homeserver names the server and the user, without the token" {
    run mn_on a1 --json homeserver
    assert_success
    assert_equal "$(jq -r .home_server <<<"$output")" "$E2E_HOMESERVER/"
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
    assert_equal "$(jq -r .token <<<"$output")" null

    run mn_on a1 homeserver
    assert_line --regexp "^ *home server +$E2E_HOMESERVER/ *\$"
    assert_line --regexp "^ *token +- *\$"
}

@test "hs is an alias of homeserver" {
    assert_equal "$(mn_on a1 --json hs)" "$(mn_on a1 --json homeserver)"
}

@test "the token needs --force" {
    run --separate-stderr mn_on a1 homeserver --token
    assert_failure
    assert_output ""
    assert_regex "$stderr" "without -f/--force"
}

@test "the token printed with --force is valid" {
    run mn_on a1 --json homeserver --token --force
    assert_success
    token=$(jq -r .token <<<"$output")

    run client_api "$token" /account/whoami
    assert_equal "$(jq -r .user_id <<<"$output")" "$alice"
}
