# List the recipes.
default:
    @just --list

# Build the release binary (the e2e suite tests this one).
build:
    cargo build --release

# Check formatting and run clippy as CI does.
lint:
    cargo fmt --check
    RUSTFLAGS=-Dwarnings cargo clippy --all-targets

# Run the unit and CLI tests.
test:
    cargo test

# Start the throwaway Synapse for the e2e suite (podman; PODMAN=docker works too).
synapse-up:
    tests/e2e/synapse.sh up

# Stop it again; it keeps no state.
synapse-down:
    tests/e2e/synapse.sh down

# Run the e2e suite against a running Synapse; `just e2e-run v3` uses /v3/sync.
e2e-run sync="sliding" *args="tests/e2e":
    MN_SLIDING_SYNC={{ if sync == "v3" { "0" } else { "" } }} bats --timing {{ args }}

# Build, start Synapse, run the e2e suite with both sync APIs, stop Synapse.
e2e: build synapse-up
    #!/usr/bin/env bash
    set -uo pipefail
    status=0
    just e2e-run sliding || status=1
    just e2e-run v3 || status=1
    just synapse-down
    exit $status

# Everything CI runs.
ci: lint test e2e
