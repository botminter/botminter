# Exploratory Test Report: Sync & Bridge Idempotency

**Date:** 2026-06-11
**Build:** bm 0.2.0-pre-alpha (88ade30-dirty) (local debug)
**Environment:** Linux x86_64, podman rootless, gh (devguyio)
**Test User:** bm-test-user@localhost (isolated)

## Results

### Phase B: Team Init + Hire

| # | Test | Result |
|---|------|--------|
| B1 | bm init (non-interactive, agentic-sdlc-minimal, tuwunel) | **PASS** |
| B2 | GitHub repo exists | **PASS** |
| B3 | GitHub project board exists | **PASS** |
| B4 | Labels created (21 labels) | **PASS** |
| B5 | Team registered in config.yml | **PASS** |
| B6 | Team repo cloned | **PASS** |
| B7 | Init again correctly rejects existing team (exit 1) | **PASS** |
| B8 | Hired alice (--reuse-app) | **PASS** |
| B9 | Hired bob (--reuse-app) | **PASS** |
| B9b | Deployed brain-prompt.md to team repo (alice and bob, per-member rendered) | **PASS** |
| B10 | Member dirs exist (engineer-alice, engineer-bob) | **PASS** |
| B11 | Hire duplicate alice correctly rejects (exit 1) | **PASS** |
| B12 | Test project repo already exists (devguyio-bot-squad/exploratory-test-project) | **PASS** |
| B13 | Added project to team (bm projects add) | **PASS** |
| B14 | Project registered in botminter.yml | **PASS** |

### Phase C: Bridge Lifecycle (Tuwunel)

| # | Test | Result |
|---|------|--------|
| C1 | First bridge start | **PASS** |
| C2 | Container running | **PASS** |
| C3 | Matrix server healthy | **PASS** |
| C3a | bm bridge identity add engineer-alice | **PASS** |
| C3b | bm bridge identity add engineer-bob | **PASS** |
| C3c | bm bridge room create exploratory-test-general | **PASS** |
| C4 | Bridge state: running, 2 member identities, 1 room | **PASS** |
| C5 | Passwords file has 3 entries | **PASS** |
| C6 | Keyring has credentials for alice + bob | **PASS** |
| C7 | Admin can login to Matrix | **PASS** |
| C8 | Room exploratory-test-general exists (!g3vyHe1djSpoPYUNO7:localhost) | **PASS** |
| C9 | Bridge start again (idempotent — already running) | **PASS** |
| C10 | Container still running | **PASS** |
| C11 | Bridge state unchanged | **PASS** |
| C12 | Alice credential unchanged after re-sync | **PASS** |
| C13 | Stopped container | **PASS** |
| C14 | Bridge start recovers stopped container | **PASS** |
| C15 | Container running again | **PASS** |
| C16 | Matrix healthy after recovery | **PASS** |
| C17 | Force-removed container | **PASS** |
| C18 | Bridge start recovers removed container | **PASS** |
| C19 | Container running after re-create | **PASS** |
| C20 | Admin login survives container re-create | **PASS** |
| C21 | Removed container + volume | **PASS** |
| C22 | Bridge start recovers from volume loss | **PASS** |
| C23 | Container running after volume re-create | **PASS** |
| C24 | Matrix healthy after volume re-create | **PASS** |
| C25 | Admin password regenerated | **PASS** |
| C25a | Re-provision alice after volume loss | **PASS** |
| C25b | Re-provision bob after volume loss | **PASS** |
| C26 | Alice: new password + keyring valid after volume re-create | **PASS** |
| C27 | Pre-existing user registered via UIAA (@engineer-pre-existing:localhost) | **PASS** |
| C28 | bridge identity add handles pre-existing user | **PASS** |
| C29 | Container stable after pre-existing user sync | **PASS** |
| C30 | Bridge state has 3 identities | **PASS** |
| C31 | Bridge start idempotent after pre-existing user | **PASS** |
| C32 | Final bridge state: running | **PASS** |
| C33 | Pre-existing user: keyring token valid (@engineer-pre-existing:localhost) | **PASS** |

### Phase D-Session: Ephemeral Session Lifecycle (all 25 ACs)

| # | Test | Result |
|---|------|--------|
| D01 | bm stop fails gracefully without daemon (AC-12) | **PASS** |
| D02 | bm status reports daemon not running (AC-12) | **PASS** |
| D03 | bm session inspect fails gracefully without daemon (AC-12) | **PASS** |
| D04 | Session started without prior bm teams sync (AC-22) | **PASS** |
| D05 | Session creation latency: 1052ms (AC-06) | **PASS** |
| D06 | Workspace marker has session_id + member fields (AC-01) | **PASS** |
| D07 | Project 'exploratory-test-project' provisioned in workspace (AC-01) | **PASS** |
| D08 | Config files (PROMPT.md, CLAUDE.md, ralph.yml) present (AC-01) | **PASS** |
| D09 | .claude/ fully assembled: agents(1), skills(3), settings.json (AC-08) | **PASS** |
| D10 | gh api user succeeds with D-02 shared GH_CONFIG_DIR (AC-09) | **PASS** |
| D11 | bm status --json has all fields: member=engineer-alice, state=Active (AC-10) | **PASS** |
| D12 | Two concurrent sessions active: alice + bob (AC-04) | **PASS** |
| D13 | Workspaces isolated: file in alice not visible in bob (AC-04) | **PASS** |
| D14 | Stopped bob selectively, alice still Active (AC-15) | **PASS** |
| D15 | Stop returned in 0s (async deactivation) (AC-19) | **PASS** |
| D16 | Force-stop session appears in bm session list as terminal (AC-15) | **PASS** |
| D17 | bm session list shows sessions with session IDs (AC-17) | **PASS** |
| D18 | bm session list shows state and finalization status columns (AC-17) | **PASS** |
| D19 | Session inspect shows ID, member, type, state, workspace (AC-18) | **PASS** |
| D20 | Session cleanup completed for 6152b5e5 (AC-18) | **PASS** |
| D21 | Bulk cleanup --all completed (AC-18) | **PASS** |
| D22 | Graceful stop finalized (completed) and branch 'finalization-test-1781137090' confirmed pushed to remote (AC-02) | **PASS** |
| D23 | Finalization results visible in inspect (AC-05) | **PASS** |
| D24 | Finalization re-trigger correctly rejected: session already Completed (AC-23) | **PASS** |
| D25 | Provision failure: non-zero exit, no partial session left (AC-07) | **PASS** |
| D27 | Crashed session workspace retained at /home/bm-test-user/.botminter/sessions/exploratory-test/engineer-alice/f744a03d (AC-26) | **PASS** |
| D26 | New session starts after crash + force-stop (AC-03) | **PASS** |
| D28 | Daemon restart: stale sessions visible in bm session list (AC-25) | **PASS** |
| D29 | Session state after start: Killed (AC-11) | **PASS** |
| D30 | Session in bm session list after force-stop (terminal state) (AC-11) | **PASS** |
| D31 | Terminal state observed via inspect: Killed (AC-11) | **PASS** |
| D32 | Session workspace retained after force-stop (retention policy) (AC-20) | **PASS** |
| D33 | Stopped session visible in bm session list (AC-20) | **PASS** |
| D34 | Individual session cleanup removed workspace (AC-21) | **PASS** |
| D35 | Work item lock lifecycle: A-acquire → B-contend(exit1) → A-release → B-acquire (AC-13) | **PASS** |
| D36 | Independent branches in isolated workspaces: alice=push-test-alice-1781137140, bob=push-test-bob-1781137140 (AC-14a) | **PASS** |
| D37 | Session inspect captures git/workspace state (AC-14b) | **PASS** |
| D38 | bm session list shows force-stopped session in output | **PASS** |
| D39 | bm session list --json has finalization_status field in all rows | **PASS** |
| D40 | bm status --history exits non-zero with migration hint to bm session list | **PASS** |
| D41 | .claude/ assembly with team-level coding-agent/ — no crash (workspace created successfully) | **PASS** |
| D42 | Lock parallel contention: exactly one session acquired (sum=1, product=0) | **PASS** |
| D43 | Lock release cycle: A-acquire → A-release → B-acquire | **PASS** |
| D44 | Lock released when session stops — B acquired after A stopped | **PASS** |

### Phase E: Member Hire & Session Workflow

| # | Test | Result |
|---|------|--------|
| E1 | Hire engineer-carol (bm_hire engineer --name carol) | **PASS** |
| E2 | bm bridge identity add engineer-carol | **PASS** |
| E3 | bm start engineer-carol: session started | **PASS** |
| E4 | Session visible in bm session list (engineer-carol) | **PASS** |
| E5 | bm bridge identity list shows engineer-carol | **PASS** |

### Phase F: Error Handling

| # | Test | Result |
|---|------|--------|
| F1 | Graceful handling when just not in PATH (output: Bridge 'tuwunel' already running.) | **PASS** |
| F2 | bm status -v works | **PASS** |
| F3 | bm members list shows 4 members | **PASS** |
| F4 | bm teams show works | **PASS** |

### Phase H: Brain Lifecycle (Chat-First Member)

| # | Test | Result |
|---|------|--------|
| H2 | No unrendered template variables in team repo brain-prompt.md | **PASS** |
| H9 | Alice and bob brain-prompt.md differ in team repo (per-member rendering) | **PASS** |
| H11 | bm start executed successfully (Started 4 member(s), skipped 0 (already running), 0 error(s). ) | **PASS** |
| H12 | brain_mode=true: active Brain session found in session registry | **PASS** |
| H13 | Without brain-prompt.md: no Brain session active (Loop sessions only) | **PASS** |
| H14 | Restored brain-prompt.md and cleaned up state | **PASS** |
| H19 | Tuwunel bridge is running (Matrix server healthy) | **PASS** |
| H20 | ACP binary available () | **PASS** |
| H21 | Admin Matrix login successful | **PASS** |
| H22 | Alice Matrix login successful | **PASS** |
| H23 | Cleaned DM room state for discovery test | **PASS** |
| H24 | Cleaned previous state for lifecycle test | **PASS** |
| H25 | bm start executed (brain mode detected) | **PASS** |
| H26 | Brain started in DM discovery mode (PID 3633915) | **PASS** |
| H27 | bm status shows brain label during lifecycle | **PASS** |
| H28 | Operator DM created and greeting sent (!IMjipaBuxDJjMJF4Il:localhost, $hHTHh9a7PxdQsRqdgwSuqUZDvY5p7dVl94blrCkVq2Q) | **PASS** |
| H28b | Brain discovered DM room (!IMjipaBuxDJjMJF4Il:localhost via dm-room.json) | **PASS** |
| H29 | Work request sent to room while brain running ($KRTWRnrPBYKEijg8c1J3zqr-C9Dh_c7DCKq1yhGhJos) | **PASS** |
| H30 | Follow-up question sent (multi-turn simulation) | **PASS** |
| H31 | Brain survived malformed/empty message (edge case) | **PASS** |
| H32 | Brain responded with meaningful content (response: Hey! I'm Alice, your engineer on the exploratory-test team. Checking my current state now....) | **PASS** |
| H29b | Brain response addresses work request (mentions project/status/tools) | **PASS** |
| H33 | User messages visible in room history (5 total messages) | **PASS** |
| H34 | DM room is private — bob is not a member of alice's DM room (expected) | **PASS** |
| H35 | Brain survived all interaction (normal + malformed + cross-member messages) | **PASS** |
| H36 | bm stop executed cleanly (exit 0) | **PASS** |
| H37 | All brain processes terminated after stop | **PASS** |
| H38 | Brain restarted successfully (recovery scenario) | **PASS** |
| H39 | Message delivered after brain restart (recovery proof, $FlZkHDBftXK4J-U6Zj_-yMjxjnt8RVDX5sKIfgha1FE) | **PASS** |
| H40 | Brain responded after recovery! NEW response detected (pre: 1, post: 2, body: Yes, fully operational. Brain restart recovered cleanly — I'm here and ready....) | **PASS** |
| H41 | Recovery start-stop cycle clean (brain lifecycle idempotent) | **PASS** |
| H42 | Status inquiry sent after brain lifecycle | **PASS** |
| H43 | All messages persist in DM room history (8 total) | **PASS** |
| H44 | dm-room.json persisted correctly (!IMjipaBuxDJjMJF4Il:localhost) | **PASS** |
| H46 | Created GitHub issue #1 for brain to discover | **PASS** |
| H47 | Brain started for task execution journey (PID 3640996) | **PASS** |
| H48 | Board check request sent to brain ($ysVO7FuGgFRSXQJFSSwy6cVOpWdw9EXjCzTwXEflXo4) | **PASS** |
| H49 | Brain acknowledged board/issue in response! (body: On it — checking the board now....) | **PASS** |
| H50 | Brain survived task execution request (PID 3640996 still alive) | **PASS** |
| H51 | Task execution journey cleaned up | **PASS** |
| H52 | Cleaned up all brain lifecycle test artifacts | **PASS** |

### Phase G: Cleanup

| # | Test | Result |
|---|------|--------|
| G1 | Removed bridge container | **PASS** |
| G2 | Removed bridge volume | **PASS** |
| G3 | Deleted GitHub repo | **PASS** |
| G4 | Deleted GitHub project | **PASS** |
| G5 | Removed local state | **PASS** |
| G6 | Cleared keyring entries | **PASS** |
| G8 | Verify clean | **FAIL** — containers='' repo=no botminter=exists |

---

## Summary

- **PASS:** 153
- **FAIL:** 1
- **NOTE:** 0
