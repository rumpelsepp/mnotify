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

# Bump the version, commit and tag it; release notes from the file NOTES, else from $EDITOR.
release version notes="":
    #!/usr/bin/env bash
    set -euo pipefail
    die() { echo "error: $*" >&2; exit 1; }

    [[ "{{ version }}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "not a version: {{ version }}"
    [[ -z "{{ notes }}" || -r "{{ notes }}" ]] || die "cannot read {{ notes }}"
    [[ "$(git branch --show-current)" == master ]] || die "not on master"
    [[ -z "$(git status --porcelain)" ]] || die "working tree is not clean"
    git fetch --quiet origin master
    [[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/master)" ]] || die "master differs from origin/master"
    ! git rev-parse -q --verify "refs/tags/v{{ version }}" >/dev/null || die "tag v{{ version }} exists"

    just lint test

    sed -i '0,/^version = ".*"/s//version = "{{ version }}"/' Cargo.toml
    cargo update --offline --workspace
    git commit -am "chore: bump version to {{ version }}"

    if [[ -n "{{ notes }}" ]]; then
        git tag -a "v{{ version }}" -F "{{ notes }}"
    else
        git tag -a "v{{ version }}" -e -m "mnotify {{ version }}"
    fi
    echo "tagged v{{ version }}; publish it with: just publish {{ version }}"

# Push the release commit and its tag, create the GitHub release from the tag message.
publish version:
    git push --atomic origin master "v{{ version }}"
    gh release create "v{{ version }}" --verify-tag --title "mnotify {{ version }}" --notes-from-tag
