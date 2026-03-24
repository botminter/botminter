# Launch Members

This guide covers provisioning workspaces for team members and launching their Ralph instances.

## Provision workspaces

Before launching members, provision their workspaces:

```bash
bm teams sync
```

This creates or updates **workspace repos** for each hired member. Each workspace repo is a dedicated git repository containing submodules for the team repo and project forks.

**New workspaces** (use `--repos` to create GitHub repos):

```bash
bm teams sync --repos
```

1. Creates a GitHub repo (`org/<team>-<member>`) for each member
2. Clones locally into `workzone/<team>/<member>/`
3. Adds the team repo as `team/` submodule
4. Adds each assigned project as `projects/<project>/` submodule
5. Checks out member branches in all submodules
6. Copies context files (AGENTS.md, PROMPT.md, ralph.yml) from `team/members/<member>/`
7. Assembles `.claude/agents/` with symlinks into `team/` submodule paths
8. Writes `.botminter.workspace` marker and `.gitignore`
9. Commits and pushes

**Existing workspaces** (without `--repos`):

```bash
bm teams sync
```

Updates submodules to latest, re-copies context files if team submodule versions are newer, and re-assembles agent dir symlinks.

## Launch all members

```bash
bm start
```

This discovers all member workspaces (via `.botminter.workspace` marker files), maps credentials from the config, and launches `ralph run -p PROMPT.md` as a background process per member. A `.topology` file is written to the team directory tracking member endpoints.

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

This shows the member table (name, role, status, branch, PID), the formation type from the topology file, and daemon status if a daemon is running.

Add `-v` for verbose output including per-member submodule status and Ralph runtime details:

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
    | Submodules (`team/`, `projects/`) | `git submodule update --remote` |
    | `AGENTS.md`, `PROMPT.md` | Re-copy if team submodule version is newer |
    | `ralph.yml` | Re-copy if team submodule version is newer |
    | `settings.local.json` | Re-copy if present |
    | `.claude/agents/` | Re-assemble symlinks into `team/` submodule paths |

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
