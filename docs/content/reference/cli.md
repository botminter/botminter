# CLI Commands

All BotMinter operations use the `bm` CLI binary. Install it with `cargo install --path crates/bm` or build with `cargo build -p bm`.

## Team creation

### `bm init`

Interactive wizard — create a new team.

```bash
bm init
```

**Behavior:**

- Prompts for workzone directory, team name, and profile
- Auto-detects GitHub auth from `GH_TOKEN` env var or `gh auth token` — prompts only if none found
- Validates the token via `gh api user` before proceeding
- Lists GitHub orgs and personal account for interactive selection
- Offers to create a new repo or select an existing one from the chosen org
- Offers to create a new GitHub Project board or select an existing one
- For new repos: extracts the profile, optionally hires members and adds projects, creates GitHub repo and pushes
- For existing repos: clones the repo (skips member/project prompts — use `bm hire` and `bm projects add` after init)
- Registers the team in `~/.botminter/config.yml` early (before label/project operations) so a failure doesn't leave config in a broken state
- Bootstraps labels (idempotent via `--force`) and creates/syncs the GitHub Project board's Status field
- Stops with actionable error messages if any GitHub operation fails
- First registered team becomes the default

## Member management

### `bm hire`

Hire a member into a role.

```bash
bm hire <role> [--name <name>] [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<role>` | Yes | Role name (must exist in the team's profile, e.g., `architect`) |
| `--name <name>` | No | Member name. Auto-generates a 2-digit suffix (e.g., `01`) if omitted |
| `-t <team>` | No | Team to operate on (defaults to default team) |

**Behavior:**

- Performs schema version guard (rejects if team schema doesn't match embedded profile)
- Extracts member skeleton from the embedded profile into `team/{role}-{name}/`
- Finalizes `botminter.yml` with the member's name
- Creates a git commit (no auto-push)
- Auto-suffix fills gaps: if `01` and `03` exist, returns `02`

### `bm members list`

List hired members for a team.

```bash
bm members list [-t <team>]
```

**Behavior:**

- Scans `team/` directory for member directories
- Displays Member, Role, and Status columns
- Status reflects running/crashed/stopped from runtime state

### `bm members show`

Show detailed information about a member.

```bash
bm members show <member> [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<member>` | Yes | Member name (e.g., `architect-01`) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Displays member name, role, and runtime status (running/crashed/stopped)
- Shows PID, start time, and workspace path if running
- Lists knowledge and invariant files for the member

### `bm roles list`

List available roles from the team's profile.

```bash
bm roles list [-t <team>]
```

## Project management

### `bm projects list`

List projects configured for the team.

```bash
bm projects list [-t <team>]
```

**Behavior:**

- Reads `botminter.yml` and displays a table of Project and Fork URL columns
- If no projects are configured, prints guidance to use `bm projects add`

### `bm projects show`

Show detailed information about a project.

```bash
bm projects show <project> [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<project>` | Yes | Project name |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Displays project name and fork URL
- Lists knowledge and invariant files under `projects/{name}/`

### `bm projects add`

Add a project to the team.

```bash
bm projects add <url> [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<url>` | Yes | Git URL of the project fork |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Derives the project name from the URL basename (strips `.git` suffix)
- Appends to `botminter.yml` projects list
- Creates `projects/{name}/knowledge/` and `projects/{name}/invariants/` directories
- Creates a git commit (no auto-push)
- Errors if the project name already exists

### `bm projects sync`

Sync the GitHub Project board's Status field options with the profile definitions, and print instructions for setting up role-based views.

```bash
bm projects sync [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Finds the team's GitHub Project board by title (`{team} Board`)
- Updates the built-in Status field options to match the profile's `statuses` definitions using the `updateProjectV2Field` GraphQL mutation
- Prints a table of role-based views with filter strings for manual setup in the GitHub UI
- Safe to re-run anytime (idempotent)

## Team management

### `bm teams list`

List all registered teams.

```bash
bm teams list
```

**Behavior:**

- Reads `~/.botminter/config.yml`
- Displays Team, Profile, GitHub, Members, Projects, and Default columns
- Member and project counts are derived from the team repo on disk

### `bm teams show`

Show detailed information about a team.

```bash
bm teams show [<name>] [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<name>` | No | Team name (uses default team if omitted) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Displays team name, profile, GitHub repo, path, and default status
- Lists hired members with their roles
- Lists configured projects with their fork URLs

### `bm teams sync`

Provision and reconcile workspaces.

```bash
bm teams sync [--push] [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--push` | No | Push team repo to GitHub before syncing |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Performs schema version guard
- Optionally pushes team repo (`git push`)
- Discovers hired members and configured projects
- For each member x project: creates or syncs a workspace
- Workspace creation: clones fork at member branch, clones team repo into `.botminter/`, surfaces files (symlinks PROMPT.md/CLAUDE.md, copies ralph.yml), assembles `.claude/agents/`, writes .gitignore and .git/info/exclude
- Workspace sync: pulls repos, re-copies changed files, re-assembles symlinks
- Reports summary: "Synced N workspaces (M created, K updated)"

## Process lifecycle

### `bm start`

Launch all members.

```bash
bm start [-t <team>] [--formation <name>]
# Alias:
bm up [-t <team>] [--formation <name>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--formation <name>` | No | Formation name (default: `local`) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Checks for `ralph` binary prerequisite
- Maps credentials from config to environment variables
- Discovers member workspaces
- Launches `ralph run -p PROMPT.md` as background process per member
- Records PIDs in `state.json` with atomic writes
- Verifies processes alive after 2 seconds
- For non-local formations: runs the formation manager as a one-shot Ralph session
- Writes a `.topology` file tracking member endpoints

### `bm stop`

Stop all members.

```bash
bm stop [-t <team>] [--force]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--force` | No | Send SIGTERM instead of graceful stop |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Graceful mode (default): runs `ralph loops stop` per member, polls for 60s
- Force mode (`--force`): sends SIGTERM immediately
- Cleans state.json entries
- Suggests `bm stop -f` on graceful failure

### `bm status`

Status dashboard.

```bash
bm status [-t <team>] [-v]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `-v` | No | Show verbose Ralph runtime details |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Header shows team name, profile, GitHub repo, and configured projects
- Displays Member, Role, Status, Started, PID table
- Shows daemon status if a daemon is running
- Checks PID liveness via `kill(pid, 0)`
- Auto-cleans crashed entries
- Verbose mode queries Ralph CLI commands per running member

## Profile commands

### `bm profiles list`

List all embedded profiles.

```bash
bm profiles list
```

**Behavior:**

- Displays Profile, Version, Schema, and Description columns
- Reads from compile-time embedded profiles

### `bm profiles describe`

Show detailed profile information.

```bash
bm profiles describe <profile>
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<profile>` | Yes | Profile name (e.g., `scrum`) |

**Behavior:**

- Displays name, version, schema, description
- Lists available roles with descriptions
- Lists all labels with descriptions

## Knowledge management

### `bm knowledge list`

List knowledge and invariant files by scope.

```bash
bm knowledge list [-t <team>] [--scope <scope>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--scope <scope>` | No | Filter by scope: `team`, `project`, `member`, `member-project` |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Lists all knowledge and invariant files in the team repo
- Filters by scope when `--scope` is provided
- Requires schema version 1.0

### `bm knowledge show`

Display the contents of a knowledge or invariant file.

```bash
bm knowledge show <path> [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<path>` | Yes | Path to a knowledge or invariant file (relative to team repo) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Validates the path is under `knowledge/` or `invariants/`
- Rejects path traversal attempts (e.g., `../`)
- Displays file contents

### `bm knowledge` (interactive)

Launch an interactive Claude Code session with the knowledge-manager skill.

```bash
bm knowledge [-t <team>] [--scope <scope>]
```

**Behavior:**

- Spawns a Claude Code session with the knowledge-manager skill injected
- Requires schema version 1.0

## Daemon

### `bm daemon start`

Start the event-driven daemon for a team.

```bash
bm daemon start [-t <team>] [--mode <mode>] [--port <port>] [--interval <interval>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--mode <mode>` | No | `webhook` or `poll` (default: `webhook`) |
| `--port <port>` | No | HTTP listener port for webhook mode (default: `8484`) |
| `--interval <interval>` | No | Poll interval in seconds for poll mode (default: `60`) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Starts a background daemon process
- **Webhook mode**: listens for GitHub webhook events on the configured port; validates signatures with HMAC-SHA256 if `webhook_secret` is set in credentials
- **Poll mode**: polls the GitHub Events API at the configured interval; tracks poll state in `~/.botminter/daemon-{team}-poll.json`
- Filters events by type: `issues`, `issue_comment`, `pull_request`
- Handles both SIGTERM and SIGINT for graceful shutdown
- Daemon log: `~/.botminter/logs/daemon-{team}.log`
- Per-member logs: `~/.botminter/logs/member-{team}-{member}.log` (each member's ralph output is separated)
- Writes PID to `~/.botminter/daemon-{team}.pid` and config to `~/.botminter/daemon-{team}.json`

### `bm daemon stop`

Stop the running daemon for a team.

```bash
bm daemon stop [-t <team>]
```

**Behavior:**

- Sends SIGTERM to the daemon process
- Waits up to 30 seconds for graceful shutdown (the daemon forwards SIGTERM to running members with a 5-second grace period)
- Escalates to SIGKILL if the daemon doesn't exit within 30 seconds
- Cleans up PID, config, and poll state files

See [Daemon Operations](daemon-operations.md) for detailed signal handling behavior and troubleshooting.

### `bm daemon status`

Show daemon status for a team.

```bash
bm daemon status [-t <team>]
```

**Behavior:**

- Reports whether the daemon is running
- Displays mode (webhook/poll), port or interval, and start timestamp

## Shell completions

### `bm completions`

Generate dynamic shell completions.

```bash
bm completions <shell>
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<shell>` | Yes | Shell to generate completions for (`bash`, `zsh`, `fish`, `powershell`, `elvish`) |

Completions are **dynamic** — tab suggestions include real values from your configuration:

- **Team names** for `-t`/`--team` flags
- **Role names** for `bm hire <role>`
- **Member names** for `bm members show <member>`
- **Profile names** for `bm profiles describe <profile>`
- **Project names** for `bm projects show <project>`
- **Formation names** for `bm start --formation <formation>`
- **Daemon modes** (`webhook`, `poll`) for `bm daemon start --mode`
- **Knowledge scopes** (`team`, `project`, `member`, `member-project`) for `bm knowledge --scope`

The generated script delegates to the `bm` binary at tab-time, so completions always reflect your current configuration.

**Setup examples:**

=== "Bash"

    ```bash
    echo 'eval "$(bm completions bash)"' >> ~/.bashrc
    ```

=== "Zsh"

    ```bash
    echo 'eval "$(bm completions zsh)"' >> ~/.zshrc
    ```

=== "Fish"

    ```bash
    bm completions fish > ~/.config/fish/completions/bm.fish
    ```

=== "PowerShell"

    ```powershell
    echo 'bm completions powershell | Invoke-Expression' >> $PROFILE
    ```

=== "Elvish"

    ```bash
    echo 'eval (bm completions elvish | slurp)' >> ~/.elvish/rc.elv
    ```

## Development commands

These are in the root Justfile for developing BotMinter itself:

```bash
just build    # cargo build -p bm
just test     # cargo test -p bm
just clippy   # cargo clippy -p bm -- -D warnings
```

## Related topics

- [Getting Started](../getting-started/index.md) — first-use walkthrough
- [Workspace Model](../concepts/workspace-model.md) — how `bm teams sync` structures workspaces
- [Generate a Team Repo](../how-to/generate-team-repo.md) — detailed `bm init` guide
- [Configuration Files](configuration.md) — daemon config, formation config, and credential fields
- [Manage Knowledge](../how-to/manage-knowledge.md) — adding and organizing knowledge files
