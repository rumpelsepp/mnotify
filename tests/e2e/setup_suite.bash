# Runs once before all tests: log in the admin that creates the test users
# and write a synadm config with its token.

setup_suite() {
    export E2E_HOMESERVER=${E2E_HOMESERVER:-http://localhost:8008}
    export E2E_SYNADM=${E2E_SYNADM:-uvx synadm@0.49.2}
    export E2E_SYNADM_CONFIG=$BATS_SUITE_TMPDIR/synadm.yaml
    local admin=${E2E_ADMIN:-@admin:localhost}
    local mn=${MN:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/target/release/mn}
    local state=$BATS_SUITE_TMPDIR/admin

    XDG_STATE_HOME=$state MN_NO_KEYRING=1 "$mn" login "$admin" \
        --homeserver "$E2E_HOMESERVER" <<<"${E2E_ADMIN_PASSWORD:-admin}" 2>/dev/null
    local token
    token=$(XDG_STATE_HOME=$state MN_NO_KEYRING=1 "$mn" --json homeserver --token --force | jq -r .token)

    cat >"$E2E_SYNADM_CONFIG" <<EOT
user: "$admin"
token: "$token"
base_url: "$E2E_HOMESERVER"
admin_path: /_synapse/admin
matrix_path: /_matrix
homeserver: ${admin#*:}
timeout: 30
ssl_verify: true
format: json
EOT
    # Fetch synadm now, not in the first test.
    $E2E_SYNADM --version >/dev/null
}
