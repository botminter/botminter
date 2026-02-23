# Daemon Operations

This page covers the internal architecture, debugging, and troubleshooting of the `bm daemon` system.

## Architecture overview

The daemon uses a **two-process model**:

1. **`bm daemon start`** — the launcher. Validates configuration, spawns the daemon process, writes the PID file, and exits.
2. **`bm daemon-run`** — the long-lived daemon process. Runs the event loop (webhook or poll), launches members one-shot, and handles signals.

```
bm daemon start          bm daemon-run (long-lived)
┌──────────────┐         ┌──────────────────────────────┐
│ Validate cfg │──spawn──▶│ Event loop                   │
│ Write PID    │         │   ├─ Receive event            │
│ Write config │         │   ├─ Launch members one-shot  │
│ Exit         │         │   └─ Wait for completion      │
└──────────────┘         └──────────────────────────────┘
```

The parent (`bm daemon start`) redirects the child's stdout/stderr to the daemon log file at `~/.botminter/logs/daemon-{team}.log`, then exits after confirming the child is alive.

## Modes of operation

### Webhook mode

Listens on a configured port (default: `8484`) for GitHub webhook HTTP POST requests. Events flow:

1. GitHub sends a POST to `http://<host>:<port>/webhook` with an `X-GitHub-Event` header
2. If a webhook secret is configured, the daemon validates the `X-Hub-Signature-256` HMAC-SHA256 signature
3. The daemon checks if the event type is relevant (`issues`, `issue_comment`, `pull_request`)
4. If relevant, it launches all members one-shot and waits for them to complete
5. Irrelevant events receive a 200 response but do not trigger member launches

```bash
bm daemon start --mode webhook --port 8484
```

Best for: production deployments with a publicly reachable endpoint or a webhook relay.

### Poll mode

Polls the GitHub Events API at a configured interval (default: `60s`). Events flow:

1. The daemon calls `gh api repos/{owner}/{repo}/events`
2. New events since the last poll are filtered by type
3. If any relevant events are found, members are launched one-shot
4. Poll state (last event ID, last poll timestamp) is persisted to `~/.botminter/daemon-{team}-poll.json`

```bash
bm daemon start --mode poll --interval 120
```

Best for: development, firewalled environments, or when webhook delivery is unreliable.

## One-shot execution model

Unlike `bm start` (which launches members as persistent background processes), the daemon uses a **one-shot** model:

1. An event arrives (webhook POST or poll detection)
2. The daemon discovers all hired members and their workspaces
3. Each member is launched with `ralph run -p PROMPT.md` — the ralph process does its work and exits
4. The daemon waits for all members to complete (interruptible by shutdown signals)
5. The cycle repeats on the next event

This eliminates idle token burn — members only run when there is work to do.

## Runtime files

| File | Path | Purpose | Lifecycle |
|------|------|---------|-----------|
| PID file | `~/.botminter/daemon-{team}.pid` | Daemon process ID | Created on start, removed on stop |
| Config JSON | `~/.botminter/daemon-{team}.json` | Mode, port, interval, start time | Created on start, removed on stop |
| Poll state JSON | `~/.botminter/daemon-{team}-poll.json` | Last event ID, last poll timestamp | Created on first poll, removed on stop |
| Daemon log | `~/.botminter/logs/daemon-{team}.log` | Daemon process output and structured log entries | Persistent, rotated at 10 MB |
| Member logs | `~/.botminter/logs/member-{team}-{member}.log` | Per-member ralph output (stdout/stderr) | Persistent, appended on each launch |

## Log files & debugging

### Daemon log

The daemon writes structured log entries to `~/.botminter/logs/daemon-{team}.log`. Each entry has the format:

```
[2026-02-22T10:30:00Z] [INFO] Daemon starting in poll mode
[2026-02-22T10:30:05Z] [INFO] Found 2 relevant event(s)
[2026-02-22T10:30:05Z] [INFO] architect-alice: launched (PID 12345)
[2026-02-22T10:30:05Z] [INFO] architect-alice: log file at ~/.botminter/logs/member-my-team-architect-alice.log
```

Log rotation happens automatically when the file exceeds 10 MB. The previous log is renamed to `daemon-{team}.log.old`.

### Per-member logs

Each member's ralph output (stdout and stderr) is redirected to its own log file:

```
~/.botminter/logs/member-{team}-{member}.log
```

This prevents output from multiple members from being interleaved. The daemon log notes the path to each member's log file when it launches.

### Tailing logs in real-time

```bash
# Watch daemon log
tail -f ~/.botminter/logs/daemon-my-team.log

# Watch a specific member's log
tail -f ~/.botminter/logs/member-my-team-architect-alice.log

# Watch all logs at once
tail -f ~/.botminter/logs/*.log
```

## Signal handling

The daemon handles two signals for graceful shutdown:

| Signal | Source | Behavior |
|--------|--------|----------|
| `SIGTERM` | `bm daemon stop`, `kill -TERM <pid>` | Sets shutdown flag, exits event loop |
| `SIGINT` | Ctrl+C (if running in foreground) | Same as SIGTERM |

### Shutdown sequence

When a shutdown signal is received:

1. The daemon's event loop detects the shutdown flag on its next iteration
2. If members are currently running (one-shot launch in progress):
   - SIGTERM is forwarded to each child process
   - The daemon waits up to 5 seconds for each child to exit
   - If a child doesn't exit within 5 seconds, it is sent SIGKILL
3. The daemon logs "Daemon stopped" and exits

### `bm daemon stop` flow

1. Reads the PID file to find the daemon process
2. Sends SIGTERM to the daemon
3. Waits up to 30 seconds for the daemon to exit (polling every second)
4. If the daemon is still alive after 30 seconds, sends SIGKILL
5. Cleans up PID, config, and poll state files

## Troubleshooting

### Daemon won't start

**"Daemon already running"**
: Another daemon instance is running for this team. Run `bm daemon stop -t <team>` first, or check for a stale PID file.

**"Failed to bind to 0.0.0.0:8484"**
: The port is already in use (another daemon or another service). Use `--port <other-port>` to pick a different port.

**"requires schema 1.0"**
: The team repo was created with an older version of `bm`. Run `bm upgrade` to migrate.

### Daemon won't stop

If `bm daemon stop` hangs or doesn't work:

```bash
# Find the daemon process
ps aux | grep 'bm daemon-run'

# Send SIGTERM manually
kill -TERM <pid>

# If that doesn't work (after 30s), send SIGKILL
kill -KILL <pid>

# Clean up stale files
rm ~/.botminter/daemon-{team}.pid
rm ~/.botminter/daemon-{team}.json
```

### Stale PID files

If the daemon crashed (e.g., the machine rebooted), the PID file may point to a dead process. Both `bm daemon start` and `bm daemon status` detect and clean up stale PID files automatically:

- **`bm daemon start`**: detects the stale PID, removes the file, and starts a fresh daemon
- **`bm daemon status`**: reports "not running (stale PID file)" and removes the file

### Members not launching

Check these in order:

1. **Event types**: The daemon only triggers on `issues`, `issue_comment`, and `pull_request` events. Other events (push, star, fork) are ignored.
2. **GitHub events**: In poll mode, verify events exist with `gh api repos/{owner}/{repo}/events | head`.
3. **gh auth**: The daemon runs `gh` commands. Verify `gh auth status` succeeds with the configured token.
4. **Member workspaces**: Run `bm teams sync` to ensure workspaces are provisioned.
5. **Daemon log**: Check `~/.botminter/logs/daemon-{team}.log` for error messages.

### Finding the right log file

| Symptom | Check |
|---------|-------|
| Daemon itself misbehaving | `~/.botminter/logs/daemon-{team}.log` |
| A specific member failing | `~/.botminter/logs/member-{team}-{member}.log` |
| Member never launched | Daemon log (look for "no workspace found" or "failed to launch") |

## Process management reference

Quick commands for operators:

```bash
# Check daemon state
bm daemon status -t <team>

# Find daemon process
ps aux | grep 'bm daemon-run'

# Watch daemon log
tail -f ~/.botminter/logs/daemon-<team>.log

# Watch member log
tail -f ~/.botminter/logs/member-<team>-<member>.log

# Graceful stop
bm daemon stop -t <team>

# Manual graceful stop
kill -TERM <pid>

# Manual force stop
kill -KILL <pid>
```

## Related topics

- [CLI Reference — Daemon](cli.md#daemon) — command syntax and flags
- [Launch Members](../how-to/launch-members.md) — choosing between persistent and daemon modes
- [Configuration Files](configuration.md) — credential fields used by the daemon
