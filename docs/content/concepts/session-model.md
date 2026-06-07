# Session Model

BotMinter uses an ephemeral session model — each time a member is started, a fresh isolated workspace is created on demand, the agent runs in it, and the workspace is cleaned up according to a configurable retention policy. No manual synchronization step is needed.

## What Is a Session?

A **session** is a single invocation of a member agent. It has:

- A unique session ID
- An isolated git worktree workspace (separate from other concurrent sessions)
- A defined lifecycle from creation through finalization and cleanup
- An exit outcome — Completed, Failed, or Killed

Sessions are created automatically by `bm start`, `bm chat`, and `bm meetings`. Operators interact with sessions via `bm status`, `bm stop`, and `bm session`.

## Session Types

| Type | Created by | Description |
|------|-----------|-------------|
| **Loop** | `bm start` | Standard Ralph Orchestrator loop member |
| **Interactive** | `bm chat`, `bm meetings` | Direct coding agent session with a persona |
| **Brain** | `bm start` (chat-first member) | Brain multiplexer — ACP session with event watcher |

## Session Lifecycle

Sessions follow a defined state machine:

```
Creating → Active → Finalizing → Completed ─┐
                  ↘ Completed (clean exit)    ├→ Retained → (cleaned up by GC)
                  ↘ Failed (remote push fail) ─┘
                  ↘ Killed (force stop)   ────┘
```

| State | Meaning |
|-------|---------|
| **Creating** | Workspace is being hydrated from shared clones |
| **Active** | Agent process is running in the workspace |
| **Finalizing** | Agent has exited; finalization subagent is committing and pushing uncommitted work |
| **Completed** | Finalization succeeded (fully or with a recovery branch); entering retention |
| **Failed** | Remote preservation was impossible; entering retention |
| **Killed** | Force-stopped; finalization was skipped; entering retention |
| **Retained** | Workspace is kept on disk for inspection; subject to retention policy |

From **Retained**, an operator can re-trigger finalization (`bm session finalize` or the finalize API) to recover work from a failed or killed session.

## The Session Daemon

The BotMinter daemon (running on port 8484 by default) manages session lifecycle:

- Creates session workspaces by hydrating git worktrees from shared bare clones
- Tracks session registry (`sessions/*.json` in the team directory)
- Launches and observes the finalization subagent after agent exit
- Runs the **retention engine** — a background task that garbage-collects expired retained sessions
- Exposes session state via HTTP API (`/api/sessions/*`)

`bm start` and `bm stop` delegate to the daemon. The daemon continues running after members stop, maintaining the session registry and retention engine.

## Retention Policy

After a session reaches a terminal state (Completed, Failed, or Killed), it enters **Retained** and the retention policy determines when the workspace is cleaned up.

| Session type | Default retention | Rationale |
|-------------|-------------------|-----------|
| Loop | Configurable (e.g. 48h) | Preserve workspace for diagnostic inspection |
| Brain | Configurable (e.g. 48h) | Same |
| Interactive (successful) | Zero (immediate) | No uncommitted work expected |
| Interactive (failed/killed) | Configurable | May have uncommitted work |

The retention engine evaluates expired sessions in the background and removes their workspaces. Operators can also trigger cleanup manually with `bm session cleanup`.

## Finalization

When an agent exits and its workspace has uncommitted or unpushed work, the finalization subagent runs in the workspace to preserve it:

| Action | Scope |
|--------|-------|
| Commit and push | Uncommitted changes on non-default branches (code, specs) |
| Commit and push | Team repo files under `specs/`, `knowledge/` |
| Push only | Committed-but-unpushed branches |
| Never touch | Credential files, auth tokens |
| Leave in place | Runtime state (logs, locks, diagnostics) |

Finalization that cannot push to the remote creates a recovery branch and opens a GitHub issue. The session transitions to **Completed** (degraded) rather than **Failed** — state is always preserved remotely.

If finalization fails entirely (remote unreachable), the session enters **Failed** and remains **Retained** for manual recovery. Re-triggering finalization is supported: `bm session finalize <session-id>` transitions a Retained session back to Finalizing.

## Viewing Sessions

```bash
bm status              # Active members with current session IDs
bm session list        # Active and terminal sessions
bm session inspect <session-id>   # Full details for a specific session
```

## Cleaning Up Sessions

To re-trigger finalization for a session that failed to finalize automatically:

```bash
bm session finalize <session-id>    # Re-trigger finalization for a retained session
```

To remove retained session workspaces from disk:

```bash
bm session cleanup <session-id>     # Remove a specific retained session workspace
bm session cleanup --all            # Remove all retained session workspaces
bm session cleanup --member <name>  # Remove retained sessions for a specific member
bm session cleanup --older-than 48h # Remove sessions older than a duration
```

## Related Topics

- [Workspace Model](workspace-model.md) — session workspace layout and shared clones
- [Architecture](architecture.md) — two-layer runtime model
- [CLI Reference](../reference/cli.md) — `bm start`, `bm stop`, `bm status`, `bm session`
- [Launch Members](../how-to/launch-members.md) — creating and stopping sessions
