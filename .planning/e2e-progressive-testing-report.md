# E2E Progressive Testing Progress Report

## Overview

We are progressively stepping through the unified operator journey e2e test suite (`scenario_operator_journey`), one case at a time. The suite runs the full `bm` CLI lifecycle twice — once fresh, once after a HOME reset — to validate idempotency and the "join existing team" flow.

## Suite Structure

54 total cases: 26 journey cases (fresh) + 1 reset_home + 26 journey cases (existing) + 1 cleanup.

The journey cases cover: init → hire → projects add → teams show → bridge identity → sync → start/stop → daemon lifecycle (poll start/verify/stop, webhook start/stop, SIGKILL escalation, stale PID, already running, crash detection).

The second pass uses `case_expect_error()` for cases that should fail when state already exists (hire_member, projects_add).

## Current State

**Cursor position:** Step 34 (`sync_bridge_and_repos_existing`) — FAILING

**State file:** `crates/bm/target/e2e-progress/scenario_operator_journey.json`
**Evidence files:** `crates/bm/target/e2e-evidence/step-*.txt`

### First pass (steps 0-25): ALL PASSED ✓

All 26 fresh journey cases pass, including daemon lifecycle cases.

### Reset HOME (step 26): PASSED ✓

Wipes HOME directory, re-installs stub ralph, profiles, git auth. Preserves tg-mock container info.

### Second pass (steps 27-33): PASSED ✓

| Step | Case | Result |
|------|------|--------|
| 27 | `init_with_bridge_existing` | ✓ Cloned existing repo |
| 28 | `hire_member_existing` | ✓ Expected error verified ("already exists") |
| 29 | `projects_add_existing` | ✓ Expected error verified ("already exists") |
| 30 | `teams_show_existing` | ✓ |
| 31 | `bridge_identity_add_existing` | ✓ |
| 32 | `bridge_identity_list_existing` | ✓ |
| 33 | `sync_bridge_and_repos_existing` | ✗ FAILING |

### Current Blocker: Step 34 — `sync_bridge_and_repos_existing`

**Symptom:** `PROMPT.md missing` — the test asserts that `PROMPT.md` exists in the workspace after sync, but it doesn't.

**Root cause chain:**

1. `bm teams sync --repos` calls `create_workspace_repo()` for `superman-alice`
2. The workspace repo already exists on GitHub (from pass 1), so it clones with `--recursive`
3. The clone succeeds but **PROMPT.md is not in the cloned repo** on GitHub
4. The workspace repo on GitHub only has: `.botminter.workspace`, `.gitmodules`, `projects/`, `team/`
5. The surfaced files (`PROMPT.md`, `AGENTS.md`, `ralph.yml`, `.claude/`) are missing from the GitHub repo

**Why files are missing from GitHub:** During the first pass, `create_workspace_repo()` calls `assemble_workspace_repo_context()` which copies files from `team/members/superman-alice/` to the workspace root. But at that point, the `team/` submodule content may be empty (submodule added but not initialized), so the source files don't exist and the copies are silently skipped (guarded by `if src.exists()`). The files only appear locally later when `sync_workspace()` runs `git submodule update --remote` and then copies them.

**The files exist locally after the first sync but were never committed+pushed to the workspace repo on GitHub.**

**Next step:** Fix the workspace creation flow so that the team submodule is fully initialized before surfacing files. This likely means running `git submodule update --init team` after `git submodule add` in `create_workspace_repo()`, or restructuring so surfacing happens after the submodule content is available.

## Bugs Found and Fixed During Testing

### 1. Relative HOME path (commit `cb4aa3d`)
`ProgressState` stored a relative `home_dir` path. Git couldn't resolve `~/.gitconfig` from a relative HOME.
**Fix:** Use `CARGO_MANIFEST_DIR` for absolute state paths.

### 2. Keyring backend: linux-native → sync-secret-service (commit `1542b9b`)
The `linux-native` feature used kernel keyutils (in-memory, lost on reboot). Tokens need to persist.
**Fix:** Switch to `sync-secret-service` which stores in the Secret Service login keyring (gnome-keyring). Added `check_keyring_unlocked()` via `dbus-secret-service` crate for actionable error messages.

### 3. Bot token assertion in bridge_functional (commit `ed8c96e`)
The test only checked that `RALPH_TELEGRAM_BOT_TOKEN` env var existed, not that it had the correct value.
**Fix:** Assert the actual token value matches `BOT_TOKEN`.

### 4. Project board cleanup timing (commit `3a3ea65`)
`cleanup_project_boards` was called in `teams_list` (first pass), deleting boards before the second pass could use them.
**Fix:** Moved to the final `cleanup` case.

### 5. Daemon find_workspace inconsistency (commit `0b51092`)
`daemon.rs:find_workspace` checked for a `.botminter` directory, but `bm teams sync` creates a `.botminter.workspace` marker file. Daemon never found workspaces.
**Fix:** Consolidated `find_workspace` and `list_member_dirs` into `workspace.rs` as single shared implementations. Both use `.botminter.workspace` marker.

### 6. bm init doesn't handle existing repos (commit `f942f03`)
`bm init --non-interactive` always tried `gh repo create`, failing with "Name already exists" on second pass.
**Fix:** Added `repo_exists()` check. If repo exists, clones it instead of creating.

### 7. git submodule add not idempotent (commit `5b53180`)
`git_submodule_add` didn't check if submodule already existed, causing "'team' already exists in the index".
**Fix:** Check `git submodule status` before attempting add.

### 8. Workspace clone missing --recursive (commit `90d5591`)
Existing workspace repos were cloned without `--recursive`, leaving submodules uninitialized.
**Fix:** Use `git clone --recursive` when the workspace repo already exists.

## Code Changes Summary

### Modified source files (bm crate):
- `crates/bm/src/commands/init.rs` — `repo_exists()`, existing repo clone path in non-interactive mode
- `crates/bm/src/commands/daemon.rs` — removed duplicate `find_workspace`, `list_member_dirs`
- `crates/bm/src/commands/start.rs` — removed duplicate `find_workspace`, `list_member_dirs`
- `crates/bm/src/completions.rs` — removed duplicate `list_member_dirs`
- `crates/bm/src/workspace.rs` — shared `find_workspace`, `list_member_dirs`, idempotent `git_submodule_add`, recursive clone for existing workspaces
- `crates/bm/src/bridge.rs` — `check_keyring_unlocked()`, `sync-secret-service` backend
- `crates/bm/Cargo.toml` — keyring features, dbus-secret-service dependency

### E2E test files:
- `crates/bm/tests/e2e/helpers.rs` — `SuiteCtx` redesign (plain struct), `GithubSuite::new_self_managed()`, `case_expect_error()`, shared helpers (stub ralph, process guards, daemon helpers), progressive state with absolute paths
- `crates/bm/tests/e2e/scenarios/operator_journey.rs` — unified 54-case suite with explicit case chains, reusable case functions, meaningful names (_fresh/_existing suffixes)
- `crates/bm/tests/e2e/scenarios/mod.rs` — single suite dispatch
- `crates/bm/tests/e2e/stub-ralph.sh` — standalone stub script with SIGTERM ignore via marker file
- `crates/bm/tests/e2e/main.rs` — progressive CLI args parsing
- `crates/bm/tests/e2e/github.rs` — `TempRepo::from_existing()`
- `crates/bm/tests/e2e/telegram.rs` — `TgMock::from_existing()`, `is_running()`, `into_parts()`
- `AGENTS.md` — progressive testing workflow documented

## How to Resume

```bash
# Check current state
cat crates/bm/target/e2e-progress/scenario_operator_journey.json

# The keyring must be unlocked for bridge_identity_add cases
# Check: dbus-send --session --dest=org.freedesktop.secrets --print-reply \
#   /org/freedesktop/secrets/collection/login \
#   org.freedesktop.DBus.Properties.Get \
#   string:"org.freedesktop.Secret.Collection" string:"Locked"

# Resume from current cursor
just e2e-step scenario_operator_journey

# Reset to re-run from a specific step (edit next_case in state file)
# Then: just e2e-step scenario_operator_journey

# Full reset (delete repo + container + state)
just e2e-reset scenario_operator_journey

# Evidence files
ls crates/bm/target/e2e-evidence/

# Always run from project root: /home/sandboxed/workspace/botminter
```

## Git Commits (chronological)

```
a4bbfbf refactor(e2e): unified operator journey suite with progressive mode
cb4aa3d fix(e2e): use absolute path for progressive state via CARGO_MANIFEST_DIR
1542b9b fix(bridge): switch keyring to sync-secret-service for persistent storage
ed8c96e test(e2e): assert correct bot token passed to ralph in bridge_functional
3a3ea65 fix(e2e): move project board cleanup to final cleanup case
52ce313 refactor(e2e): daemon cases use ctx.home instead of ephemeral tempdirs
041414d refactor(e2e): split daemon start/stop into separate cases
db074bb refactor(e2e): split daemon poll into start/verify/stop for progressive inspection
6a273d4 refactor(e2e): extract stub ralph to standalone script file
b660727 refactor(e2e): merge stub ralph scripts, use env var for SIGTERM ignore
e62cd0f fix(e2e): use marker file instead of env var for SIGTERM ignore
0b51092 refactor: consolidate find_workspace and list_member_dirs into workspace.rs
f942f03 fix(init): handle existing repo in non-interactive mode
182ee77 fix(init): allow existing directory when joining existing repo (REVERTED)
99fb134 Revert "fix(init): allow existing directory when joining existing repo"
0a7c517 refactor(e2e): explicit case chains with reusable functions
5b53180 fix(workspace): skip submodule add when already registered
90d5591 fix(workspace): clone existing workspace repos with --recursive
```
