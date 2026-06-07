# Bridge Setup

This guide walks you through setting up a bridge for your team, from initialization to running members with bridge credentials. Matrix (Tuwunel) is the default bridge and recommended starting point.

## Prerequisites

- A profile with bridge support (`agentic-sdlc-minimal` supports Matrix, Telegram, and Rocket.Chat)
- For local bridges (Matrix, Rocket.Chat): Podman installed and running
- For external bridges (Telegram, experimental): bot tokens created via @BotFather
- `bm` CLI installed and GitHub auth configured

## Step 1: Initialize with bridge selection

During `bm init`, select a bridge when prompted:

```bash
bm init
```

The wizard lists bridges from the selected profile plus a "No bridge" option. Matrix (Tuwunel) is pre-selected as the default.

For non-interactive/CI mode, use the `--bridge` flag:

```bash
# Matrix via Tuwunel (default -- BotMinter manages the homeserver)
bm init --non-interactive \
  --profile agentic-sdlc-minimal \
  --team-name my-team \
  --org my-org \
  --repo my-team \
  --bridge tuwunel

# Telegram (experimental -- you manage the bots)
bm init ... --bridge telegram

# Rocket.Chat (experimental -- BotMinter manages the server)
bm init ... --bridge rocketchat
```

Omitting `--bridge` means no bridge is configured. When a bridge is selected, the init output will suggest provisioning bridge identities with `bm bridge identity add`.

## Step 2: Start local bridges

For local bridges (Rocket.Chat, Tuwunel), start the service before provisioning identities:

```bash
bm bridge start
```

This starts the Podman container(s) for the bridge. The command is idempotent -- if the bridge is already running, it verifies health and skips the restart.

External bridges (Telegram) skip this step -- the service is managed externally.

## Step 3: Provision bridge identities

Provision identities and rooms on the bridge:

```bash
bm bridge identity add <member>
```

This is idempotent -- it only provisions members not yet onboarded. For external bridges, it prompts for a token. For local bridges, it creates bot accounts and generates tokens automatically.

## Step 4: Launch members

Start your team as usual:

```bash
bm start
```

BotMinter resolves per-member credentials from the keyring (or environment variables) and injects them into each member's Ralph process. For local bridges, `bm start` also verifies the bridge is running (and starts it if needed).

You can also start or stop individual members:

```bash
bm start engineer-01    # start a single member
bm stop engineer-01     # stop a single member (bridge stays running)
```

## Managing credentials

### Adding credentials after hire

If you need to add credentials for a member:

```bash
bm bridge identity add engineer-01
# External bridges: prompts for token
# Local bridges: auto-provisions via the bridge API
```

After adding credentials, the member's `ralph.yml` is updated with `RObot.enabled: true`. Changes take effect on the next session start.

### Viewing stored credentials

Check what's stored for a member:

```bash
bm bridge identity show engineer-01            # masked token
bm bridge identity show engineer-01 --reveal   # full token
```

### Rotating credentials

Generate a fresh token for a member:

```bash
bm bridge identity rotate engineer-01
```

The new token is stored in the keyring, replacing the old one.

## Checking bridge status

View the current bridge state, including provisioned identities and rooms:

```bash
bm bridge status
```

## CI and headless environments

When the system keyring is unavailable, supply credentials via environment variables:

```bash
export BM_BRIDGE_TOKEN_ENGINEER_01=your-bot-token
bm start
```

The naming convention is `BM_BRIDGE_TOKEN_{USERNAME}` with the username uppercased and hyphens replaced by underscores.

## Bridge-specific notes

### Matrix / Tuwunel (local, default)

The Tuwunel bridge runs a local Matrix homeserver in a Podman container. All configuration is via environment variables:

| Variable | Default | Purpose |
|----------|---------|---------|
| `TUWUNEL_PORT` | `8008` | Host port for the homeserver |
| `TUWUNEL_IMAGE` | `ghcr.io/matrix-construct/tuwunel:latest` | Container image |
| `TUWUNEL_SERVER_NAME` | `localhost` | Matrix server name (appears in user IDs like `@bot:localhost`) |
| `TUWUNEL_REG_TOKEN` | `bm-tuwunel-reg-default` | Registration token for user creation |
| `TUWUNEL_ADMIN_USER` | `bmadmin` | Admin account username |
| `TUWUNEL_ADMIN_PASS` | `bmadmin-pass-default` | Admin account password |

Lifecycle:

1. **Start:** `bm bridge start` creates a Podman container (`bm-tuwunel-{team}`) with a persistent volume (`bm-tuwunel-{team}-data`). An admin account (`bmadmin` by default) is registered automatically as the first user.
2. **Provision:** `bm bridge identity add <member>` registers per-member bot users via the Matrix client-server API. Passwords are generated automatically and stored in `tuwunel-passwords.json` (see [Security considerations](../concepts/bridges.md#security-considerations)).
3. **Rooms:** A default `{team}-general` room is created automatically during provisioning. All provisioned bots are invited to it. Create additional rooms with `bm bridge room create <name>`.
4. **Connect a client:** Point any Matrix client (e.g., [Element](https://element.io)) at `http://127.0.0.1:8008` (or your custom `TUWUNEL_PORT`). Sign in as `bmadmin` with the admin password to observe agent conversations. Search for `#{team}-general:localhost` in the room directory.
5. **Launch:** `bm start` injects per-member access tokens as `RALPH_MATRIX_ACCESS_TOKEN` + `RALPH_MATRIX_HOMESERVER_URL`.
6. **Stop:** `bm bridge stop` stops the container. Data persists in the Podman volume for restart.

!!! note "Local-only homeserver"
    Tuwunel runs with federation disabled and binds to `127.0.0.1` by default.
    The default admin password is not secret -- override `TUWUNEL_ADMIN_PASS` if the
    host is shared or network-accessible.

Requires: Podman, `curl`, `jq`, `openssl`.

### Telegram (external, experimental)

1. **Create bots:** Create one Telegram bot per team member via [@BotFather](https://t.me/BotFather). Each bot needs a unique name.
2. **Supply tokens:** Provide the bot token via `bm bridge identity add`.
3. **Validate:** Run `bm bridge identity show <member>` to verify tokens are stored correctly.
4. **Launch:** `bm start` injects per-member bot tokens as `RALPH_TELEGRAM_BOT_TOKEN`.

### Rocket.Chat (local, experimental)

1. **Start:** `bm bridge start` creates a Podman pod with Rocket.Chat + MongoDB.
2. **Provision:** `bm bridge identity add <member>` creates bot users and generates auth tokens via the RC REST API.
3. **Rooms:** A default room is created automatically. Create additional rooms with `bm bridge room create <name>`.
4. **Launch:** `bm start` injects per-member auth tokens as `RALPH_ROCKETCHAT_AUTH_TOKEN` + `RALPH_ROCKETCHAT_SERVER_URL`.
5. **Stop:** `bm bridge stop` stops the Podman pod (data is preserved for restart).

Requires: Podman, `curl`, `jq`.

## Related topics

- [Bridge Concepts](../concepts/bridges.md) -- bridge types, credential flow, security model
- [CLI Reference](../reference/cli.md) -- full command documentation
- [Launch Members](launch-members.md) -- detailed launch workflow
