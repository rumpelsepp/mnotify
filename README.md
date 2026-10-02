# mnotify

`mnotify` is a small command line client for [Matrix](https://matrix.org),
built for one job: **getting messages from scripts, cron jobs and headless
servers into a Matrix room**, end-to-end encrypted rooms included. It can read
too, so simple bots are a shell loop away.

The binary is called `mn`. It prints tables and plain text for people and,
with `--json`, JSON for scripts; logs go to stderr and failures exit non-zero,
so it composes with `jq` and `set -e`.

```
$ backup.sh || echo "Backup on $(hostname) failed" | mn send -r '#ops:example.org'
$Kx0…
```

## Features

`mnotify` is deliberately not a full chat client: no UI, no calls, no room
management beyond creating and joining rooms. What it does support, using the categories of the
[client list on matrix.org](https://matrix.org/ecosystem/clients/):

| Feature | Status | Notes |
|---|---|---|
| End-to-end encryption | ✅ supported | Send to and read from encrypted rooms (incl. attachments), emoji verification, key backup and recovery |
| SSO | ✅ supported | Legacy `m.login.sso` (SAML, OIDC upstreams) via `--sso`, also headless over an SSH port forward |
| OAuth 2.0 / OIDC | ◐ partial | `--qr`: login by scanning a QR code with an already signed-in device (MSC4108). No browser-based OAuth flow yet |
| Threads | ✅ supported | Start or continue a thread with `send --thread` (text, notices, files), read one with `messages --thread`; replies stay in their thread |
| Spaces | ✗ | Spaces are flagged in `mn room list`, nothing more |
| Multiple accounts | ✅ supported | One login per profile, select with `-p`/`--profile`; also several devices of one account side by side |
| Invisible crypto (MSC4153) | ✅ supported | Room keys only for cross-signed devices, messages from other devices are ignored; `mn` cross-signs a new account itself |
| Sliding sync | ✅ supported | Simplified sliding sync (MSC4186) where the homeserver offers it, `/v3/sync` otherwise |
| Voice / video calls | ✗ | Out of scope |
| Custom emoji / image packs | ✗ | Out of scope |
| Multiple UI languages | ✗ | English only |

On top of that, the parts that matter for automation: JSON on stdout, room
aliases everywhere, `--json` output, Markdown, notices, emotes, replies, mentions that ping
people's phones, file and image
attachments with thumbnails, redactions, reading via `messages` and `sync`,
concurrent invocations that queue up instead of corrupting each other, and secrets in the system keyring or a `0600` file.

## Showcase: alerts from a headless server in five minutes

The typical setup is a dedicated bot account that lives on the server and
posts into a room you are in.

**1. Create a bot account** on your homeserver (e.g. `@backupbot:example.org`)
with Element or your server's admin tools.

**2. Log in on the server.** A server usually has no desktop keyring; `mn`
notices that and keeps its secrets in a file readable only by you:

```
$ mn login @backupbot:example.org
password:
warning: no system keyring in use; the access token and the store passphrase are kept in plain text in ~/.local/state/mnotify/@backupbot:example.org/session.json (mode 0600). Anyone who can read this file can act as @backupbot:example.org.
```

In provisioning scripts, pass the password on stdin (`mn login … < pwfile`).
There is deliberately no command-line option for it: it would end up in `ps`
and the shell history.

**3. Invite the bot** to your room in Element, then accept the invite on the
server. Aliases and room IDs both work everywhere:

```
$ mn room join '#ops:example.org'
!abc…:example.org
```

**4. Send.** Text as argument or on stdin, optionally as Markdown, optionally
as a notice (`m.notice`, the Matrix convention for automated messages:
clients render it less prominently and bots must not reply to it):

```
$ mn send -r '#ops:example.org' "Hello from $(hostname)"
$ df -h / | mn send -r '#ops:example.org' --notice
$ mn send -r '#ops:example.org' -n -m "**backup failed** on \`nas\`"
$ mn send -r '#ops:example.org' --attachment /var/log/backup.log
```

Set `MN_ROOM` once instead of repeating `-r`:

```
$ export MN_ROOM='#ops:example.org'
$ uptime | mn send
```

### Recipe: wake someone up

Messages notify according to each reader's settings; a mention makes it a
ping (highlight and sound under the default push rules). Mention people explicitly, or
the whole room with `--mention-room`, which needs the power level for `@room`
notifications (50 by default):

```
$ mn send --mention @alice:example.org "backup failed on nas"
$ mn send --mention-room -n "maintenance in 10 minutes"
```

Names in the text never ping anyone: `mn` always declares its mentions
(`m.mentions`, Matrix 1.7), so a log line that happens to contain someone's
name stays quiet.

### Recipe: one thread per job

`send` prints the new event ID. Use it as the root of a thread that collects
the details, so the room itself only shows one line per job:

```
$ id=$(mn send "Deploy of v1.4 started")
$ ./deploy.sh > deploy.log 2>&1; mn send --thread "$id" -a deploy.log
$ mn send --thread "$id" -n "Deploy finished ✅"
$ mn messages --thread "$id"
```

`--thread` accepts the root or any event in the thread; `--reply-to` answers a
single message instead.

### Recipe: a tiny command bot

`mn sync` runs forever and prints each incoming event; with `--json`, one
JSON object per line:

```sh
mn sync -r '#ops:example.org' --json |
  jq --unbuffered -r 'select(.type == "m.room.message" and .sender != "@backupbot:example.org")
                      | .content.body' |
  while read -r body; do
      [ "$body" = "!disk" ] && df -h / | mn send -n
  done
```

## Logging in

Always use the complete Matrix ID including the domain, e.g.
`@user:example.org`. `mn` finds the homeserver via the domain's
`.well-known/matrix/client`, falling back to `https://example.org`. If your
homeserver lives elsewhere and has no `.well-known`, name it:

```
$ mn login @user:example.org --homeserver https://matrix.example.org
```

Each profile holds one login; `mn logout` ends the session on the server and
deletes the local state of that profile. Without `-p`/`--profile` (or
`MN_PROFILE`), `mn` uses the profile `default`. Any other name selects a profile
of its own, with its own login, store and lock:

```
$ mn login @alerts:example.org
$ mn -p ops login @ops:example.org
$ mn -p ops send --room '#ops:example.org' 'deploy done'
```

A profile can also hold a second device of the same account, e.g. one that
sends while `mn sync` runs in the default profile (see
[Concurrent invocations](#concurrent-invocations)). Profile names consist of
ASCII letters, digits, `-`, `_` and `.`, and do not start with `.`.

Logins made with older `mn` versions are not migrated: `mn` asks you to remove
them with `mn clean @user:example.org` and to log in again.

### Password

```
$ mn login @user:example.org
```

`mn` reads the password interactively, or from stdin when it is not a
terminal.

### QR code (OAuth 2.0 / MAS homeservers)

Homeservers backed by a next-generation auth server (OAuth 2.0 / MAS, e.g.
matrix.org) no longer accept password logins from new clients. Instead:

```
$ mn login @user:example.org --qr
```

`mn` prints a QR code in the terminal (this works over SSH). Open an already
signed-in Element, choose "Link new device" / "Sign in with QR code", scan the
terminal, and type the two-digit check code Element shows back into `mn`.

### SSO / SAML

If your homeserver has an SSO button on the Element login page (enterprise
SAML, OIDC, ...):

```
$ mn login @user:example.org --sso
```

`mn` prints the homeserver's SSO URL and waits for the browser redirect on a
local port. Open the URL, sign in, done. On a headless box, forward the port
first (the command prints the exact `ssh -L ...` line). Use `--idp <id>` to
skip the server's identity-provider picker.

## Encrypted rooms

`mn` follows [MSC4153](https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/4153-invisible-crypto.md)
("invisible crypto"): it shares room keys only with cross-signed devices and
ignores encrypted messages from devices that are not cross-signed. Element does
the same in its "exclude insecure devices" mode. A device that is not
cross-signed therefore cannot take part in encrypted rooms, and `mn` refuses to
send there from one.

Messages sent before a device logged in are decrypted with room keys from the
server-side key backup, once recovery is set up on the device. As in Element,
the backup cannot prove which device a key came from, so the sender of these
messages is not authenticated; `messages` and `sync` mark them `[unverified]`
(Element shows a grey shield).

**New account** (e.g. a dedicated bot account): `mn login` creates the
cross-signing keys and signs its device itself. Then store them in secret
storage once, so that later logins can be signed as well, and keep the printed
recovery key:

```
$ mn recovery enable
EsT ...
```

**Account that already has cross-signing** (e.g. set up in Element): `mn login`
warns that the new device is not cross-signed. Sign it with the recovery key...

```
$ mn recovery recover < recovery-key.txt
```

...or verify it from another signed-in device: run `mn verify` and leave it
running. The other device shows no prompt by itself; open its session list
(in Element: Settings > Sessions), select the mnotify session (the device ID
`mn verify` prints) and choose "Verify". Then compare the emojis on both sides
and confirm. `mn verify --device <DEVICE_ID>` starts it from this side
instead.

`mn recovery status` reports the current state.

## Command overview

| Command | Purpose |
|---|---|
| `mn login` / `mn logout` | Create / end the session |
| `mn room create` | Create a room (encrypted unless `--unencrypted`), optionally with `--name`, `--alias`, `--invite`; prints the room ID |
| `mn room join <room>` | Join a room or accept an invite |
| `mn send` | Send text, Markdown, notices, emotes, replies or files; prints the event ID |
| `mn messages -r <room>` | Print the latest messages of a room (`--json`: an array of raw events) |
| `mn sync` | Print incoming timeline events, forever (`--json`: one raw event per line) |
| `mn room list`, `mn room info -r <room>` | Your rooms / one room's details: name, members, encryption, ... |
| `mn redact` | Delete an event |
| `mn typing` | Show / hide the typing indicator |
| `mn verify`, `mn recovery` | Device verification and key backup |
| `mn whoami`, `mn homeserver` | Account and server info |
| `mn clean <user>` | Delete local state without contacting the server |

`mn <command> --help` documents every option. Output is meant for people:
tables, plain text, and just the ID for `send`, `room create` and `room join`, so
`id=$(mn send …)` works as is. Add `--json` to any command for JSON instead. Every command that takes a room
accepts a room ID (`!abc:example.org`) or an alias (`#ops:example.org`).

With `-m`/`--markdown` the body is rendered as Markdown (the message keeps a
plain-text fallback for clients that don't render it). Images sent with
`--attachment` get their dimensions and, above 800px, a thumbnail, so clients
show an inline preview.

## Technical Stuff

### Build

```
$ cargo build --release
```

Requires Rust 1.93 or newer (Rust edition 2024, matrix-sdk 0.18). Use a release
build: a debug build is large (see
[#18](https://github.com/rumpelsepp/mnotify/issues/18)) and takes seconds to
open the encrypted store on every invocation. TLS is always
[rustls](https://github.com/rustls/rustls).

### Tests

The [justfile](justfile) has the common entry points (`just` lists them):

```
$ just lint       # cargo fmt --check, clippy
$ just test       # unit and CLI tests
$ just e2e        # build, start Synapse, run the e2e suite twice, stop Synapse
$ just ci         # all of the above
```

`tests/e2e` holds a [bats](https://bats-core.readthedocs.io/) suite that runs
the release binary against a throwaway Synapse in a podman container
(`PODMAN=docker` works too). It needs `bats`, `bats-assert`, `bats-support`,
`jq`, `curl` and [`uv`](https://docs.astral.sh/uv/) (test users are created
with [synadm](https://codeberg.org/synadm/synadm) via `uvx`). It runs once
with sliding sync and once with `/v3/sync`; while Synapse is up
(`just synapse-up`), `just e2e-run v3 tests/e2e/room-create.bats` runs a
single file.

There is one file per command, named after it (`mn room create` is
`room-create.bats`); `mn-*.bats` cover what applies to all commands
(`--json`, profiles, concurrent invocations, other global options). Every test creates its own users and keeps each device's state in its own
temporary `XDG_STATE_HOME`, so all tests share one homeserver but no state.

### Concurrent invocations

Only one `mn` process per profile and account runs at a time; others wait
until it exits. Each process keeps the account's Olm sessions in memory, and two of
them encrypting with the same session at once make the receiver lose room
keys, i.e. messages it can never decrypt. Overlapping cron jobs or a burst of
`mn send` from a script are therefore serialized; with `-vv`, a waiting process
logs the PID it waits for, and after ten seconds it warns anyway.

`mn sync` and `mn verify` hold the lock for as long as they run, so a script
that reacts to `mn sync` output with `mn send` in the same profile blocks. Log
in a second device of the account for the sender in a profile of its own
(`mn -p sender login ...`) and send with `mn -p sender send ...`.

### Environment Variables

#### `MN_ROOM`

Default room for every command that takes `-r`/`--room`.

#### `MN_NO_KEYRING`

`mnotify` stores the session (access/refresh token) and the passphrase of the
encrypted state store in the system keyring via the
[Secret Service API](https://specifications.freedesktop.org/secret-service/latest/).
If no keyring is usable at the first login (no session bus or no Secret
Service, as on most servers), it falls back to
`$XDG_STATE_HOME/mnotify/$PROFILE/$USER_ID/session.json` (mode `0600`) automatically,
and `mn login` prints a warning. Once that file exists it is always used. Set
`MN_NO_KEYRING` to use the file even where a keyring is available.

Anyone who can read that file can act as the account, so use a dedicated bot
account and a dedicated system user.

If you logged in where a keyring was available (e.g. on the desktop) and later
run `mn` where it is not (e.g. over SSH without a session bus), `mn` refuses to
continue rather than creating a new store passphrase. Run it from the desktop
session, or start over with `mn clean` and log in again.

#### `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`

Standard proxy variables, honoured for all Matrix requests (`socks5://`
proxies included).

#### `MN_SLIDING_SYNC`

Set to `0` during `mn login` to make that login use `/v3/sync` even where the
homeserver offers sliding sync. The choice holds until `mn logout`.

#### `MN_INSECURE`

Disable TLS verification. Only for testing.

#### `MN_PROFILE`

Default for `-p`/`--profile`.

#### `RUST_LOG`

Standard [`tracing`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)
filter, e.g. `RUST_LOG=matrix_sdk=debug`. Overrides `-v`/`-q`.

### Files

`mnotify` conforms to the
[XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html).

All state lives in one directory per profile, `$XDG_STATE_HOME/mnotify/$PROFILE/`
(`$PROFILE` is `default` without `-p`):

- `$PROFILE/meta.json` -- which user is logged in, and its homeserver URL.
- `$PROFILE/$USER_ID/session.json` -- session + store passphrase, only if no keyring is used.
- `$PROFILE/$USER_ID/store/` -- the SQLite state/crypto store, encrypted with the store passphrase.
- `$PROFILE/$USER_ID/lock` -- held by the running `mn` process, see [Concurrent invocations](#concurrent-invocations).

In the system keyring, the secrets are stored under the service `mnotify`
(profile `default`) or `mnotify/$PROFILE`, with the user ID as the account.

State from an `mn` before profiles (`meta.json` and the user directories
directly in `$XDG_STATE_HOME/mnotify/`) is moved into the profile `default` on
the first run; `mn` says so on stderr.
