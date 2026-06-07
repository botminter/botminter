# Launch Members

This guide covers launching team member sessions and managing their lifecycle.

## Launch all members

```bash
bm start
```

This creates ephemeral session workspaces for each hired member, assembles configuration files from the team repo, and launches each member as a background process. Members with a `brain-prompt.md` file run in **brain mode** (chat-first) — an ACP-based multiplexer that monitors loops, responds on the bridge, and picks up work autonomously. Standard members launch `ralph run -p PROMPT.md` directly. A `.topology` file is written to the team directory tracking member endpoints.

The `bm up` alias also works:

```bash
bm up
```

### Launch with a formation

Specify a formation to control the deployment target:

```bash
bm start --formation local    # Default — launches locally
bm start --formation k8s      # Delegates to the formation manager via a one-shot Ralph session
```

Non-local formations (e.g., `k8s`) require a configured formation manager in the profile's `formations/` directory.

## Check status

```bash
bm status
```

This shows active sessions (member name, role, session state, elapsed time) and daemon status.

Add `-v` for verbose output including per-session workspace paths and Ralph runtime details:

```bash
bm status -v
```

View completed session history:

```bash
bm status --history
```

## Stop members

Graceful stop (triggers finalization for uncommitted work):

```bash
bm stop
```

Force stop (skips finalization):

```bash
bm stop --force
```

Stopping also removes the `.topology` file from the team directory.

## Event-driven daemon

Instead of running members continuously, use the daemon to launch members one-shot when GitHub events arrive. This eliminates idle token burn:

```bash
bm daemon start                         # Webhook mode (default, port 8484)
bm daemon start --mode poll --interval 120  # Poll mode, check every 2 minutes
```

Check daemon status and stop:

```bash
bm daemon status
bm daemon stop
```

The daemon filters for `issues`, `issue_comment`, and `pull_request` events. When an event arrives, it discovers members, spawns them one-shot, waits for completion, and cleans up. Each member's output is written to a separate log file at `~/.botminter/logs/member-{team}-{member}.log`. See [CLI Reference — Daemon](../reference/cli.md#daemon) for full options and [Daemon Operations](../reference/daemon-operations.md) for architecture, debugging, and troubleshooting.

## Configuration propagation

When team configuration changes (new knowledge, updated prompts, modified `ralph.yml`), changes take effect automatically on the next session start. Stop current sessions and start new ones:

```bash
bm stop && bm start
```

Each new session assembles fresh configuration from the latest team repo state — no manual synchronization step is needed.

## Launch for a specific team

All commands accept `-t` to target a specific team (defaults to the default team):

```bash
bm start -t my-other-team
bm status -t my-other-team
bm stop -t my-other-team
```

## Troubleshooting

**"Member not found"**
: Run `bm hire <role>` first to add a member.

**Changes to `ralph.yml` not taking effect**
: Stop current sessions with `bm stop` and start new ones with `bm start`. Configuration is assembled fresh each session.

## Related topics

- [Manage Members](manage-members.md) — hiring and configuring members
- [Workspace Model](../concepts/workspace-model.md) — session workspace layout and lifecycle
- [CLI Reference](../reference/cli.md) — full command documentation
- [Configuration Files](../reference/configuration.md) — daemon config, formation config, and topology file
