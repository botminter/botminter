# Launch Members

This guide covers provisioning workspaces for team members and launching their Ralph instances.

## Provision workspaces

Before launching members, provision their workspaces:

```bash
bm teams sync
```

This performs the following steps for each hired member × configured project:

1. Clones the project repo into a workspace directory (e.g., `workzone/my-team/human-assistant/`)
2. Clones the team repo into `.botminter/` inside the workspace
3. Writes a `.botminter/.member` marker file
4. Creates symlinks for `PROMPT.md` and `CLAUDE.md`
5. Copies `ralph.yml` and `settings.local.json`
6. Assembles `.claude/agents/` from all agent layers (team, project, member)
7. Sets up `.git/info/exclude` and `.gitignore` to exclude workspace files

Use `--push` to push the team repo before syncing:

```bash
bm teams sync --push
```

## Launch all members

```bash
bm start
```

This discovers all member workspaces, maps credentials from the config, and launches `ralph run -p PROMPT.md` as a background process per member. A `.topology` file is written to the team directory tracking member endpoints.

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

This shows the member table (name, role, status, PID), the formation type from the topology file, and daemon status if a daemon is running.

Add `-v` for verbose Ralph runtime details:

```bash
bm status -v
```

## Stop members

Graceful stop (waits up to 60 seconds):

```bash
bm stop
```

Force stop (sends SIGTERM immediately):

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

## Re-sync after changes

If team configuration has changed (new knowledge, updated prompts, modified `ralph.yml`), re-sync workspaces:

```bash
bm teams sync
```

??? note "What sync updates"
    | What | How |
    |------|-----|
    | Team repo (`.botminter/`) | `git pull` |
    | Project repo | `git pull` |
    | `ralph.yml` | Re-copy if source is newer |
    | `settings.local.json` | Re-copy if source is newer |
    | `.claude/agents/` | Re-assemble symlinks |
    | `PROMPT.md`, `CLAUDE.md` | Verify and repair symlinks |
    | `.git/info/exclude` | Verify and add missing patterns |

After syncing, restart agents for `ralph.yml` changes to take effect:

```bash
bm stop && bm start
```

## Launch for a specific team

All commands accept `-t` to target a specific team (defaults to the default team):

```bash
bm start -t my-other-team
bm status -t my-other-team
bm stop -t my-other-team
```

## Troubleshooting

**"No workspaces found"**
: Run `bm teams sync` first to provision workspaces.

**"Member not found"**
: Run `bm hire <role>` first to add a member.

**Changes to `ralph.yml` not taking effect**
: Run `bm teams sync` and restart agents with `bm stop && bm start`. `ralph.yml` is a copy, not a symlink.

**Symlinks broken after moving directories**
: Run `bm teams sync` to repair.

## Related topics

- [Manage Members](manage-members.md) — hiring and configuring members
- [Workspace Model](../concepts/workspace-model.md) — workspace layout and file surfacing
- [CLI Reference](../reference/cli.md) — full command documentation
- [Configuration Files](../reference/configuration.md) — daemon config, formation config, and topology file
