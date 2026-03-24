# Rocket.Chat + Podman Pod Spike

**Date:** 2026-03-10
**Status:** PASSED -- full bidirectional human-in-the-loop validated

## What Was Tested

1. **Podman Pod Infrastructure** -- RC 8.2.0 + MongoDB 7.0 in a single pod
2. **REST API Operations** -- Admin login, bot user creation, channel creation, bidirectional messaging via curl
3. **Real Ralph Orchestrator** -- Ralph 2.7.0 with `RObot.rocketchat` backend against the live RC instance

## Results

### Podman Pod (spike.sh)

| Step | Result | Notes |
|------|--------|-------|
| MongoDB 7.0 start + replica set init | PASS | ~4s to be ready |
| Rocket.Chat 8.2.0 boot | PASS | ~42s to become healthy |
| Admin login via REST API | PASS | |
| Bot user creation (users.create) | PASS | |
| Bot token generation (users.createToken) | PASS | Requires `CREATE_TOKENS_FOR_USERS_SECRET` env var on server (RC 8.x change) |
| Channel creation (channels.create) | PASS | Bot auto-joined via members array |
| Bot channel visibility (channels.list) | PASS | |
| Bidirectional messaging via curl | PASS | Bot sends, admin replies, bot retrieves history |
| /status command message transit | PASS | Command-shaped messages pass through RC transport |

### Ralph Orchestrator -- Full Bidirectional Loop (ralph-rc-test/)

| Step | Result | Notes |
|------|--------|-------|
| Ralph starts with RObot.rocketchat | PASS | Correctly uses RC service, not Telegram |
| Greeting sent to RC channel | PASS | "Ralph loop `main` connected via Rocket.Chat" |
| Claude agent runs inside Ralph | PASS | Agent reads PROMPT.md, creates tasks, emits human.interact |
| ralph emit "human.interact" | PASS | Event emitted, question posted to RC channel |
| RC channel shows question | PASS | "What guidance do you have for this test?" |
| Admin responds via RC REST API | PASS | "Confirm the default test flow is fine." |
| Ralph receives human response | PASS | Picked up in iteration 2 |
| guidance-received.txt written | PASS | Contains: "Confirm the default test flow is fine." |
| LOOP_COMPLETE emitted | PASS | Loop terminated normally after 2 iterations |
| Farewell sent on exit | PASS | "Ralph loop `main` disconnecting." |
| RC channel shows full flow | PASS | greeting -> question -> answer -> farewell |

**Full loop timeline (2 iterations, 1m 57s):**
1. Iteration 1: Ralph boots, connects to RC, Claude agent emits `human.interact`
2. Ralph waits for human response (blocking, 120s timeout)
3. Admin replies in RC channel via REST API
4. Iteration 2: Ralph picks up response, Claude agent writes `guidance-received.txt`, emits `LOOP_COMPLETE`
5. Ralph sends farewell, shuts down cleanly

## Observations

### Timing
- MongoDB startup: ~4 seconds
- Rocket.Chat startup: ~40 seconds (includes MongoDB connection, initial setup)
- Total pod boot to API-ready: ~45 seconds
- Ralph startup to first RC message: <1 second

### RC 8.2.0 API Changes
- **`users.createToken`** now requires a `secret` parameter matching the `CREATE_TOKENS_FOR_USERS_SECRET` env var set on the server. This is a security enhancement in RC 8.x.
- **2FA/TOTP** is enabled by default. Must be disabled via `OVERWRITE_SETTING_Accounts_TwoFactorAuthentication_Enabled=false` env var at startup.
- **`/api/v1/info`** returns 404 in RC 8.2.0. Use **`/api/info`** instead for health checks.
- **IPv6 connectivity** issues with localhost -- use `127.0.0.1` explicitly for reliable connections.

### Ralph Version Requirement
- **Ralph 2.6.0 has a config validation bug**: when `RObot.rocketchat` is configured (without telegram), validation still requires `RALPH_TELEGRAM_BOT_TOKEN`. This blocks RC-only usage.
- **Ralph 2.7.0 fixes this bug**: config validation correctly passes when only rocketchat is configured.
- The bridge integration must ensure Ralph >= 2.7.0 or the operator must set a dummy `RALPH_TELEGRAM_BOT_TOKEN`.

### Transport Layer (RC-06)
- Ralph's `ralph-rocketchat` crate successfully sends messages to a real RC instance
- Messages appear in the RC channel from the bot user
- The RC polling service starts and stops cleanly
- **Full bidirectional loop proven**: agent question -> human answer -> agent writes file -> loop completes
- Command handler correctness is covered by Ralph's 129 internal tests in the `ralph-rocketchat` crate

## Environment
- Rocket.Chat: 8.2.0 (registry.rocket.chat/rocketchat/rocket.chat:latest)
- MongoDB: 7.0.30
- Ralph Orchestrator: 2.7.0
- Podman: 5.7.1
- Platform: linux/amd64 (Fedora 43)

## Files
- `spike.sh` -- Standalone script to boot RC + MongoDB pod, create users/channels, test messaging
- `ralph-rc-test/` -- Minimal Ralph workspace for live RC integration test
  - `ralph.yml` -- RObot.rocketchat config
  - `PROMPT.md` -- Test agent instructions
  - `AGENTS.md` -- Test agent context

## Cleanup
```bash
podman pod stop bm-rc-spike && podman pod rm bm-rc-spike
```
