---
phase: 10-rocket-chat-bridge
plan: 01
subsystem: infra
tags: [rocket-chat, podman, mongodb, ralph-rocketchat, spike, bidirectional-messaging]

# Dependency graph
requires:
  - phase: 09-profile-integration
    provides: bridge abstraction and profile integration validated with Telegram
provides:
  - RC + MongoDB Podman Pod spike script (spike.sh)
  - Ralph Orchestrator RC integration proof (full bidirectional loop)
  - RC 8.2.0 API observations and workarounds documented
affects: [10-02-PLAN, 10-03-PLAN]

# Tech tracking
tech-stack:
  added: [rocket-chat-8.2.0, mongodb-7.0, podman-pod]
  patterns: [podman-pod-infrastructure, rc-rest-api-auth, create-tokens-for-users-secret]

key-files:
  created:
    - .planning/spikes/rc-podman-spike/spike.sh
    - .planning/spikes/rc-podman-spike/ralph-rc-test/ralph.yml
    - .planning/spikes/rc-podman-spike/ralph-rc-test/PROMPT.md
    - .planning/spikes/rc-podman-spike/ralph-rc-test/AGENTS.md
    - .planning/spikes/rc-podman-spike/README.md
  modified: []

key-decisions:
  - "RC 8.2.0 users.createToken requires CREATE_TOKENS_FOR_USERS_SECRET env var on server"
  - "RC 8.2.0 2FA must be disabled via OVERWRITE_SETTING env vars for bot auth"
  - "Use /api/info not /api/v1/info for RC 8.2.0 health checks"
  - "Use 127.0.0.1 not localhost to avoid IPv6 connectivity issues"
  - "Ralph >= 2.7.0 required for RC-only RObot config (2.6.0 has validation bug)"

patterns-established:
  - "Podman Pod pattern: RC + MongoDB in single pod with port mapping"
  - "RC admin bootstrap: OVERWRITE_SETTING_* env vars for non-interactive setup"
  - "Bot token via CREATE_TOKENS_FOR_USERS_SECRET for programmatic credential generation"

requirements-completed: [RC-03, RC-06]

# Metrics
duration: 36min
completed: 2026-03-10
---

# Phase 10 Plan 01: RC + Podman Pod Spike Summary

**Full bidirectional Ralph + Rocket.Chat human-in-the-loop proven via Podman Pod with RC 8.2.0 + MongoDB 7.0**

## Performance

- **Duration:** 36 min
- **Started:** 2026-03-10T13:10:00Z
- **Completed:** 2026-03-10T13:46:35Z
- **Tasks:** 3
- **Files created:** 5

## Accomplishments
- Podman Pod infrastructure validated: RC 8.2.0 + MongoDB 7.0 boots in ~45s with full REST API access
- Bidirectional REST API messaging proven via curl: bot sends, admin replies, bot retrieves both
- Full Ralph Orchestrator human-in-the-loop loop proven: agent emits human.interact -> question posted to RC channel -> admin responds -> Ralph receives response -> writes guidance-received.txt -> LOOP_COMPLETE (2 iterations, 1m 57s)
- RC 8.2.0 API changes documented (createToken secret, 2FA, health check URL, IPv6)
- Ralph 2.6.0 config validation bug identified; 2.7.0 required for RC-only usage

## Task Commits

Each task was committed atomically:

1. **Task 1: Create RC Podman Pod spike script** - `19b9f1a` (feat)
2. **Task 2: Ralph Orchestrator RC integration workspace** - `3408cf7` (feat)
3. **Task 2b: Spike README with observations** - `09a598b` (docs)
4. **Task 2c: Full bidirectional loop proof** - `10aa167` (feat)

## Files Created/Modified
- `.planning/spikes/rc-podman-spike/spike.sh` - Standalone script: boot RC+MongoDB pod, create users/channels, test bidirectional messaging
- `.planning/spikes/rc-podman-spike/ralph-rc-test/ralph.yml` - Ralph RObot.rocketchat config for live RC integration
- `.planning/spikes/rc-podman-spike/ralph-rc-test/PROMPT.md` - Test agent instructions (send question, write response, complete)
- `.planning/spikes/rc-podman-spike/ralph-rc-test/AGENTS.md` - Test agent context
- `.planning/spikes/rc-podman-spike/README.md` - Full spike results, observations, and environment details

## Decisions Made
- **CREATE_TOKENS_FOR_USERS_SECRET**: RC 8.2.0 requires this env var for programmatic bot token generation via users.createToken. The bridge must set this on the RC container.
- **2FA disabled via env vars**: RC 8.2.0 enables 2FA by default. OVERWRITE_SETTING_Accounts_TwoFactorAuthentication_Enabled=false and OVERWRITE_SETTING_Accounts_TwoFactorAuthentication_By_Email_Enabled=false must be set at container startup.
- **Health check endpoint**: /api/v1/info returns 404 in RC 8.2.0; use /api/info instead.
- **IPv4 explicit**: Use 127.0.0.1 instead of localhost to avoid IPv6 connection reset issues.
- **Ralph version**: Ralph >= 2.7.0 required. Version 2.6.0 has a config validation bug that requires RALPH_TELEGRAM_BOT_TOKEN even when only RObot.rocketchat is configured.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed MongoDB health check grep pattern**
- **Found during:** Task 1 (spike.sh)
- **Issue:** MongoDB 7.0 `mongosh --quiet --eval "db.runCommand({ping:1})"` outputs `{ ok: 1 }` (no quotes around key), not `"ok" : 1`
- **Fix:** Changed grep pattern from `'"ok" : 1\|"ok":1'` to `'ok.*1'`
- **Files modified:** .planning/spikes/rc-podman-spike/spike.sh
- **Verification:** MongoDB detected as up on attempt 2
- **Committed in:** 19b9f1a (Task 1 commit)

**2. [Rule 1 - Bug] Fixed RC health check endpoint for RC 8.2.0**
- **Found during:** Task 1 (spike.sh)
- **Issue:** `/api/v1/info` returns 404 in Rocket.Chat 8.2.0
- **Fix:** Changed health check URL to `/api/info`
- **Files modified:** .planning/spikes/rc-podman-spike/spike.sh
- **Committed in:** 19b9f1a (Task 1 commit)

**3. [Rule 1 - Bug] Fixed IPv6 connectivity issue**
- **Found during:** Task 1 (spike.sh)
- **Issue:** `curl http://localhost:3100` connects via IPv6 (::1) which gets connection reset by Podman
- **Fix:** Changed RC_URL from `http://localhost:${RC_PORT}` to `http://127.0.0.1:${RC_PORT}`
- **Files modified:** .planning/spikes/rc-podman-spike/spike.sh
- **Committed in:** 19b9f1a (Task 1 commit)

**4. [Rule 1 - Bug] Fixed RC 8.2.0 users.createToken API change**
- **Found during:** Task 1 (spike.sh)
- **Issue:** RC 8.2.0 requires `secret` parameter matching `CREATE_TOKENS_FOR_USERS_SECRET` env var
- **Fix:** Added `CREATE_TOKENS_FOR_USERS_SECRET` env var to RC container and pass secret in createToken call
- **Files modified:** .planning/spikes/rc-podman-spike/spike.sh
- **Committed in:** 19b9f1a (Task 1 commit)

**5. [Rule 1 - Bug] Fixed RC 8.2.0 2FA blocking bot login**
- **Found during:** Task 1 (spike.sh)
- **Issue:** RC 8.2.0 enables 2FA by default, blocking programmatic login
- **Fix:** Added `OVERWRITE_SETTING_Accounts_TwoFactorAuthentication_Enabled=false` env var to RC container
- **Files modified:** .planning/spikes/rc-podman-spike/spike.sh
- **Committed in:** 19b9f1a (Task 1 commit)

**6. [Rule 3 - Blocking] Upgraded Ralph from 2.6.0 to 2.7.0**
- **Found during:** Task 2 (Ralph RC integration)
- **Issue:** Ralph 2.6.0 config validation requires RALPH_TELEGRAM_BOT_TOKEN even when only RObot.rocketchat is configured
- **Fix:** Built and installed Ralph 2.7.0 from source (/opt/workspace/ralph-orchestrator)
- **Verification:** ralph preflight --format json shows config: pass with RC-only config
- **Committed in:** 3408cf7 (Task 2 commit, documented in README)

---

**Total deviations:** 6 auto-fixed (5 bugs, 1 blocking)
**Impact on plan:** All fixes necessary for correct operation with RC 8.2.0 and Ralph 2.7.0. No scope creep.

## Issues Encountered
- Rocket.Chat 8.2.0 has significant API changes from previous versions (createToken, 2FA, health endpoint). All resolved via env var configuration.
- Ralph 2.6.0 has a config validation bug for RC-only RObot configs. Resolved by building and installing 2.7.0 from local source.

## User Setup Required

None - spike is self-contained with Podman Pod infrastructure.

## Next Phase Readiness
- Infrastructure pattern proven: Podman Pod with RC + MongoDB boots reliably
- REST API operations validated: user creation, channel creation, messaging
- Ralph's ralph-rocketchat crate confirmed working against real RC 8.2.0
- RC 8.2.0 API changes documented for bridge implementation (Plan 02)
- Ready to proceed to Plan 02: RC bridge files + bridge-type-aware credential injection

---
*Phase: 10-rocket-chat-bridge*
*Completed: 2026-03-10*
