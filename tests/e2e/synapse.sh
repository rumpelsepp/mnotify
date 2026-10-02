#!/usr/bin/env bash
# Start, stop or inspect the throwaway Synapse of the e2e suite.
#
#   tests/e2e/synapse.sh up | down | logs
#
# Uses podman; set PODMAN=docker to use Docker instead.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
podman=${PODMAN:-podman}
name=${E2E_SYNAPSE_NAME:-mnotify-e2e-synapse}
port=${E2E_SYNAPSE_PORT:-8008}
image=localhost/mnotify-e2e-synapse

up() {
    "$podman" build --quiet --tag "$image" "$here/synapse" >/dev/null
    "$podman" rm --force --ignore "$name" >/dev/null 2>&1 || true
    "$podman" run --detach --rm --name "$name" \
        --publish "127.0.0.1:$port:8008" "$image" >/dev/null

    local url="http://127.0.0.1:$port/_matrix/client/versions"
    for _ in $(seq 120); do
        if curl --silent --fail --output /dev/null "$url"; then
            # The admin the suite creates its users with (via synadm).
            "$podman" exec "$name" register_new_matrix_user \
                --config /data/homeserver.yaml --user admin --password admin \
                --admin http://localhost:8008 >/dev/null
            echo "synapse is up on http://localhost:$port, admin: @admin:localhost"
            return
        fi
        sleep 0.5
    done
    echo "synapse did not come up within 60 s" >&2
    "$podman" logs "$name" >&2 || true
    exit 1
}

case ${1:-} in
up) up ;;
down) "$podman" stop --ignore "$name" >/dev/null ;;
logs) "$podman" logs "$name" ;;
*)
    echo "usage: $0 up|down|logs" >&2
    exit 2
    ;;
esac
