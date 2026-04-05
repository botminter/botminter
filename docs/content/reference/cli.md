# CLI Commands

BotMinter provides two CLI binaries: `bm` (operator-facing) and `bm-agent` (agent-facing). Install both with `cargo install --path crates/bm` or build with `cargo build -p bm`.

## Team creation

### `bm init`

Interactive wizard — create a new team.

```bash
bm init
```

**Behavior:**

- Prompts for workzone directory, team name, and profile
- Requires existing GitHub auth (`gh auth login` or `GH_TOKEN` env var) — fails if no session found
- Validates credentials via `gh api user` before proceeding
- Lists GitHub organizations for interactive selection (personal accounts are blocked — GitHub App identity requires an org)
- Offers to create a new repo or select an existing one from the chosen org
- Offers to create a new GitHub Project board or select an existing one
- If the selected profile supports bridges, prompts for bridge selection (or "No bridge")
- For new repos: resolves the default coding agent from the profile, extracts the profile (filtering agent tags and renaming `context.md` to the agent's context file), optionally hires members and adds projects, creates GitHub repo and pushes
- For existing repos: clones the repo (skips member/project prompts — use `bm hire` and `bm projects add` after init)
- Registers the team in `~/.botminter/config.yml` early (before label/project operations) so a failure doesn't leave config in a broken state
- Bootstraps labels (idempotent via `--force`) and creates/syncs the GitHub Project board's Status field
- Stops with actionable error messages if any GitHub operation fails
- First registered team becomes the default

### Non-interactive mode

For scripted or CI usage:

```bash
bm init --non-interactive --profile <profile> --team-name <name> --org <org> --repo <repo> [--bridge <name>] [--project <url>] [--workzone <path>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--non-interactive` | Yes | Run without interactive prompts |
| `--profile <profile>` | Yes | Profile name (e.g., `scrum-compact`) |
| `--team-name <name>` | Yes | Team identifier |
| `--org <org>` | Yes | GitHub organization (personal accounts not allowed) |
| `--repo <repo>` | Yes | GitHub repo name |
| `--bridge <name>` | No | Bridge to configure (e.g., `telegram`). Must be supported by the profile. Omit for no bridge |
| `--project <url>` | No | Project fork URL to add, or `new` to create a GitHub Project board |
| `--workzone <path>` | No | Override workzone directory (default: `~/.botminter/workspaces`) |
| `--credentials-file <path>` | No | Import App credentials from a YAML file (for machine migration) |

**Behavior:**

- Runs the full init flow without prompts
- Requires existing GitHub auth (`gh auth login` or `GH_TOKEN`) — validates that `--org` is a real GitHub Organization
- Creates the GitHub repo, bootstraps labels, creates a Project board, and registers the team
- In non-interactive mode, profile version mismatches are auto-resolved (same as `--force`)

## Environment management

### `bm env create`

Prepare the runtime environment for a team. Delegates to `formation.setup()`, which verifies prerequisites for the active formation type.

```bash
bm env create [-t <team>] [--formation <name>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `-t <team>` | No | Team to prepare the environment for (defaults to default team) |
| `--formation <name>` | No | Formation to use (default: `local`) |

**Prerequisites:**

- A team must exist (created via `bm init`)

**Behavior:**

- For **local** formations: verifies that the local runtime prerequisites are available (currently `ralph`; local bridge workflows also need `just` when used)
- For future formation types (Lima, K8s): provisions the required infrastructure
- Idempotent: re-running succeeds without errors

**Example:**

```bash
bm env create -t my-team
```

### `bm env delete`

Tear down the runtime environment.

```bash
bm env delete [<name>] [--force] [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<name>` | No | VM name to delete (required for Lima environments) |
| `--force` | No | Skip confirmation prompt |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- For **local** formations: nothing to tear down (informational message)
- For **Lima** environments: stops and deletes the VM, removes from config
- Prompts for confirmation when deleting VMs unless `--force` is given

**Example:**

```bash
bm env delete
bm env delete bm-alpha --force
```

### `bm runtime` (deprecated)

The `bm runtime create` and `bm runtime delete` commands are deprecated in favor of `bm env create` and `bm env delete`. They continue to work as hidden aliases for backward compatibility.

### `bm attach`

Open an interactive shell in the team's formation environment.

```bash
bm attach [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `-t <team>` | No | Team to operate on |

**Behavior:**

- For **v2 teams** (with formations dir): delegates to `formation.shell()`.
    - **Local formation:** Returns an error — you are already in the local environment. Use your terminal directly.
    - **Lima/remote formations:** Opens an interactive shell (SSH, kubectl exec, etc.) into the formation's environment.
- For **v1 teams** (no formations dir, Lima VMs): legacy behavior using `limactl shell`.
    - Resolves the target VM using 3-step resolution:
        1. If `-t <team>` is given and that team has a `vm` field set → uses that VM
        2. If exactly one VM is registered in config → uses it automatically
        3. If multiple VMs exist → prompts interactively (errors in non-interactive/piped contexts)
    - Requires `limactl` installed and at least one VM registered (via `bm env create`)
    - Checks VM status, offers to start stopped VMs
    - Execs into `limactl shell <vm-name>`

**Example:**

```bash
# Attach to a specific team's environment
bm attach -t my-team
```

## Member management

### `bm hire`

Hire a member into a role.

```bash
bm hire <role> [--name <name>] [-t <team>] [--reuse-app --app-id <id> --client-id <id> --private-key-file <path> --installation-id <id>] [--save-credentials <path>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<role>` | Yes | Role name (must exist in the team's profile, e.g., `architect`) |
| `--name <name>` | No | Member name. Auto-generates a 2-digit suffix (e.g., `01`) if omitted |
| `-t <team>` | No | Team to operate on (defaults to default team) |
| `--reuse-app` | No | Use pre-existing GitHub App credentials instead of creating a new App |
| `--app-id <id>` | With `--reuse-app` | GitHub App ID |
| `--client-id <id>` | With `--reuse-app` | GitHub App Client ID |
| `--private-key-file <path>` | With `--reuse-app` | Path to the App's PEM private key file |
| `--installation-id <id>` | With `--reuse-app` | GitHub App Installation ID |
| `--save-credentials <path>` | No | Save App credentials to file during creation |

**Behavior:**

- Performs schema version guard (rejects if team schema doesn't match profile)
- Extracts member skeleton from the profile on disk into `members/{role}-{name}/`
- Finalizes `botminter.yml` with the member's name
- **GitHub App setup:** Creates a new GitHub App for the member via the manifest flow (browser-based), or stores pre-existing credentials with `--reuse-app`. App credentials are saved in the system keyring.
- Ensures the App is installed on the team repo and all project repos
- If the team has an external bridge configured, prompts for an optional bridge token (interactive mode only). The token is stored in the system keyring.
- Creates a git commit (no auto-push)
- Auto-suffix fills gaps: if `01` and `03` exist, returns `02`
- **Idempotent:** Re-hiring an existing member without `--reuse-app` creates a new App (replacement). With `--reuse-app`, stores the provided credentials (reconnect).

### `bm fire`

Fire a member — stop, uninstall App, remove credentials and directories.

```bash
bm fire <member> [-t <team>] [--keep-app]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<member>` | Yes | Member name to fire (e.g., `superman-01`) |
| `-t <team>` | No | Team to operate on (defaults to default team) |
| `--keep-app` | No | Preserve the GitHub App installation for reuse |

**Behavior:**

Executes a 6-step teardown sequence:

1. **Stop member** — via Team → Formation → Daemon pipeline (graceful stop)
2. **Uninstall App** — signs JWT from stored credentials, calls `DELETE /app/installations/{id}` (skipped with `--keep-app`)
3. **Remove credentials** — deletes all App credential keys from the system keyring
4. **Remove member directory** — deletes `members/{member}/` from the team repo
5. **Remove member workspace** — deletes the workspace directory under the workzone
6. **Print instructions** — shows URL for manual App deletion via GitHub UI (App itself can't be deleted via API)

Steps execute sequentially. On partial failure, the command reports what succeeded and what failed — no rollback. Re-running `bm fire` for a partially cleaned-up member is safe.

### `bm credentials export`

Export all members' credentials to a YAML file for machine migration.

```bash
bm credentials export -o <file> [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `-o <file>` | Yes | Output file path |
| `-t <team>` | No | Team to operate on (defaults to default team) |

**Behavior:**

- Enumerates all hired members from the team repo
- Reads GitHub App credentials (app_id, client_id, private_key, installation_id) from the system keyring
- Reads bridge credentials (token) if a bridge is configured
- Writes YAML file with 0600 permissions
- Prints security warning to stderr

**Output format:**

```yaml
team: my-team
members:
  superman:
    github_app:
      app_id: "123456"
      client_id: "Iv1.abc123"
      private_key: |
        -----BEGIN RSA PRIVATE KEY-----
        ...
        -----END RSA PRIVATE KEY-----
      installation_id: "789012"
    bridge:
      token: "syt_xxxxxxxxx"
```

**Machine migration workflow:**

```bash
# Old machine:
bm credentials export -o team-creds.yml

# New machine:
bm init --credentials-file team-creds.yml
bm teams sync -a
```

### `bm members list`

List hired members for a team.

```bash
bm members list [-t <team>]
```

**Behavior:**

- Scans `members/` directory for member directories
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
- Shows workspace repo URL, branch, and per-submodule status (up-to-date/behind/modified) when workspace exists
- Shows resolved coding agent
- Lists knowledge and invariant files for the member

### `bm roles list`

List available roles from the team's profile.

```bash
bm roles list [-t <team>]
```

## Interactive sessions

### `bm chat`

Start an interactive chat session with a team member.

```bash
bm chat <member> [-t <team>] [--hat <hat>] [--render-system-prompt]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<member>` | Yes | Member name (e.g., `architect-01`) |
| `--hat <hat>` | No | Restrict to a specific hat (e.g., `executor`, `designer`) |
| `--render-system-prompt` | No | Print the generated system prompt and exit (no chat session) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Resolves the member's workspace and reads `ralph.yml` (guardrails, hat instructions) and `PROMPT.md` from it
- Builds a meta-prompt with role identity, hat capabilities, guardrails, role context, and reference paths
- **Three modes:**
    - `bm chat <member>` — hatless mode: agent has awareness of all hats, human drives the workflow
    - `bm chat <member> --hat executor` — hat-specific mode: agent is in character as that hat
    - `bm chat <member> --render-system-prompt` — prints the generated system prompt to stdout and exits (for debugging/inspection). Works with `--hat` too.
- In normal mode: writes the meta-prompt to a temp file and launches the coding agent via `formation.exec_in()` (v2 teams) or direct process exec (v1 teams)
- Requires a workspace created by `bm teams sync`

### `bm minty`

Launch Minty, the BotMinter interactive assistant.

```bash
bm minty [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `-t <team>` | No | Team to operate on (gives Minty team-specific context) |

**Behavior:**

- Launches a coding agent session with Minty's persona prompt, running in the current working directory
- Minty is a thin persona shell — not a team member, not a Ralph instance — just a coding agent session primed with BotMinter knowledge
- **Works without any teams configured**: if `~/.botminter/` doesn't exist, Minty runs in "profiles-only" mode and can browse profiles and answer general questions, but team-specific commands are unavailable
- **Auto-initializes**: if Minty's config (`~/.config/botminter/minty/`) is not present, it is automatically extracted from the `bm` binary
- **Agent resolution**:
    - With `-t`: uses the team's configured coding agent
    - Without `-t`: uses the default coding agent from the first available profile on disk
- Uses `--append-system-prompt-file` to inject Minty's persona prompt, giving it higher authority than `CLAUDE.md`

**Examples:**

```bash
# Launch Minty in the current directory (no team needed)
bm minty

# Launch Minty with team-specific context
bm minty -t my-team
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
- Installs all hired members' GitHub Apps on the new project repo (skips members without credentials)
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

- Displays team name, profile, profile source path, GitHub repo, path, and default status
- Shows the resolved coding agent
- Lists hired members with their roles
- Lists configured projects with their fork URLs

### `bm teams sync`

Provision and reconcile workspaces.

```bash
bm teams sync [--repos] [--bridge] [--all|-a] [-v] [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--repos` | No | Push team repo to GitHub before syncing; also creates workspace repos on GitHub for new members |
| `--bridge` | No | Provision bridge identities and rooms on the bridge |
| `--all` / `-a` | No | Equivalent to `--repos --bridge` (all remote operations) |
| `-v` | No | Show detailed sync status per workspace (submodule updates, file copy decisions, branch state) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Performs schema version guard
- Optionally pushes team repo (`git push`)
- Discovers hired members and configured projects
- For each member: creates or syncs a workspace repo
- New workspace: creates a git repo with `team/` submodule (and `projects/<name>/` submodules), copies context files, assembles agent dir, writes `.gitignore` and `.botminter.workspace` marker
- Existing workspace: updates submodules to latest, checks out member branches, re-copies context files when newer, re-assembles agent dir symlinks, commits and pushes changes
- Reports summary: "Synced N workspaces (M created, K updated)"

## Process lifecycle

### `bm start`

Launch members (all, or a specific one).

```bash
bm start [<member>] [-t <team>] [--formation <name>] [--no-bridge] [--bridge-only]
# Alias:
bm up [<member>] [-t <team>] [--formation <name>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<member>` | No | Start only this member (starts all if omitted) |
| `--formation <name>` | No | Formation name (default: `local`) |
| `--no-bridge` | No | Skip bridge auto-start |
| `--bridge-only` | No | Start bridge only, do not launch members |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- For local bridges: auto-starts the bridge if not already running (skipped when starting a single member)
- If bridge is already running, verifies health and skips restart
- Checks for `ralph` binary prerequisite
- Resolves per-member credentials from keyring or environment variables
- Discovers member workspaces
- Detects chat-first members (brain mode) via `brain-prompt.md` in workspace
- For brain members: launches the brain multiplexer (`bm brain-run`) which runs an ACP session with event watcher and heartbeat
- For standard members: launches `ralph run -p PROMPT.md` as background process
- Records PIDs and mode (brain/ralph) in `state.json` with atomic writes
- Verifies processes alive after 2 seconds
- For non-local formations: runs the formation manager as a one-shot Ralph session
- Writes a `.topology` file tracking member endpoints

### `bm stop`

Stop members (all, or a specific one).

```bash
bm stop [<member>] [-t <team>] [-f|--force] [--bridge] [--all]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<member>` | No | Stop only this member (stops all if omitted) |
| `-f` / `--force` | No | Send SIGTERM instead of graceful stop |
| `--bridge` | No | Also stop the bridge service |
| `--all` | No | Full teardown: stop members, daemon, and bridge |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Graceful mode (default): sends SIGTERM to brain members (multiplexer handles shutdown), runs `ralph loops stop` for standard members, polls for 60s
- Force mode (`--force`): sends SIGTERM immediately to all members
- Cleans state.json entries
- Suggests `bm stop -f` on graceful failure
- When stopping a single member, bridge lifecycle is not affected
- Bridge is left running unless `--bridge` or `--all` is passed (prints a reminder to use `bm stop --bridge`)
- `--all` stops members via the daemon, then shuts down the daemon itself, and stops the bridge

### `bm status`

Status dashboard.

```bash
bm status [-t <team>] [-v]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `-v` | No | Show verbose workspace submodule status and Ralph runtime details |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Header shows team name, profile, GitHub repo, and configured projects
- Displays Member, Role, Status, Branch, Started, PID table
- Status column shows "brain" for chat-first members, "running" for standard members, "crashed" or "stopped" as appropriate
- Branch column shows the workspace repo's current git branch (or "—" if no workspace exists)
- Shows daemon status if a daemon is running
- Checks PID liveness via `kill(pid, 0)`
- Auto-cleans crashed entries
- Verbose mode shows per-member submodule status (up-to-date/behind/modified) and queries Ralph CLI commands per running member

## Profile commands

### `bm profiles init`

Extract built-in profiles and Minty config to disk.

```bash
bm profiles init [--force]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--force` | No | Overwrite all existing profiles without prompting |

**Behavior:**

- Extracts all built-in profiles from the `bm` binary to `~/.config/botminter/profiles/`
- Extracts Minty interactive assistant config to `~/.config/botminter/minty/`
- Fresh install (no existing profiles on disk): extracts all without prompting
- Existing profiles with `--force`: overwrites all silently
- Existing profiles without `--force`: prompts per-profile ("Overwrite `<name>`? [y/N]", defaults to No/skip)
- New profiles (added in a newer `bm` version but not yet on disk) are always extracted without prompting
- Minty config is always extracted/updated on every init run

!!! tip "Usually not needed"
    You rarely need to run this command manually. If profiles are missing when you run any profile-reading command (`bm init`, `bm hire`, `bm profiles list`, etc.), BotMinter auto-prompts to initialize them. Use `bm profiles init --force` to reset profiles to their built-in defaults after customization, or to pick up new profiles from a newer `bm` version.

### `bm profiles list`

List available profiles.

```bash
bm profiles list
```

**Behavior:**

- Displays Profile, Version, Schema, and Description columns
- Reads from disk at `~/.config/botminter/profiles/`
- If profiles are not yet initialized, auto-prompts to extract them (see [`bm profiles init`](#bm-profiles-init))

### `bm profiles describe`

Show detailed profile information.

```bash
bm profiles describe <profile> [--show-tags]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<profile>` | Yes | Profile name (e.g., `scrum`) |
| `--show-tags` | No | Show coding-agent dependent files — files containing inline agent tags |

**Behavior:**

- Displays name, version, schema, description
- Lists available roles with descriptions
- Lists all labels with descriptions
- Lists configured coding agents with their file conventions (context_file, agent_dir, binary)
- With `--show-tags`: scans profile files and lists those containing inline agent tags (e.g., `<!-- +agent:claude-code -->`), showing which agents are referenced in each file

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

## Bridge management

!!! warning "Experimental"
    Bridge commands are experimental and may change between releases.

### `bm bridge start`

Start a local bridge service.

```bash
bm bridge start [-t <team>]
```

**Behavior:**

- Starts the bridge's Podman container(s) via the Justfile `start` recipe
- Runs a health check after start
- Saves bridge state (service URL, status) to `bridge-state.json`
- Idempotent: if the bridge is already running and healthy, skips the start
- Only applies to local bridges (Tuwunel, Rocket.Chat). External bridges print a message and exit.

### `bm bridge stop`

Stop a local bridge service.

```bash
bm bridge stop [-t <team>]
```

**Behavior:**

- Stops the bridge's Podman container(s) via the Justfile `stop` recipe
- Updates bridge state to "stopped"
- Data is preserved for restart (Podman volumes / pods are not deleted)

### `bm bridge status`

Show the current bridge state for the team.

```bash
bm bridge status [--reveal] [-t <team>]
```

| Parameter | Description |
|-----------|-------------|
| `--reveal` | Show sensitive information (operator password and token) |
| `-t <team>` | Team to operate on (default team if omitted) |

**Behavior:**

- Displays bridge type, service URL (if running), provisioned identities, and rooms
- With `--reveal`: also shows operator password (for Matrix client login) and access token
- Reads from `bridge-state.json` in the team directory

### `bm bridge identity add`

Add or update a bridge identity for a member.

```bash
bm bridge identity add <username> [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<username>` | Yes | Member name (e.g., `superman-01`) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Prompts for a bridge token (external bridges) or auto-provisions (local bridges)
- Stores the credential in the system keyring
- Updates `bridge-state.json` with the identity record

### `bm bridge identity show`

Show stored credentials for a bridge identity.

```bash
bm bridge identity show <username> [--reveal] [-t <team>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<username>` | Yes | Member name (e.g., `superman-01`) |
| `--reveal` | No | Show full token (default: masked) |
| `-t <team>` | No | Team to operate on |

**Behavior:**

- Displays username, user ID, and creation timestamp from `bridge-state.json`
- Retrieves the token from the system keyring and displays it (masked by default, full with `--reveal`)
- If no token is in the keyring, shows the environment variable name to use as fallback

### `bm bridge identity rotate`

Rotate credentials for an existing bridge identity.

```bash
bm bridge identity rotate <username> [-t <team>]
```

**Behavior:**

- Generates or accepts new credentials for the member
- Updates the system keyring with the new token
- Updates `bridge-state.json`

### `bm bridge identity remove`

Remove a bridge identity for a member.

```bash
bm bridge identity remove <username> [-t <team>]
```

**Behavior:**

- Removes the credential from the system keyring
- Removes the identity from `bridge-state.json`

### `bm bridge room create`

Create a room on the bridge platform.

```bash
bm bridge room create <room-name> [-t <team>]
```

**Behavior:**

- Creates a room/channel on the configured bridge
- Records the room in `bridge-state.json`

### `bm bridge room list`

List rooms managed by the bridge.

```bash
bm bridge room list [-t <team>]
```

**Behavior:**

- Lists all rooms/channels tracked in `bridge-state.json`
- For local bridges, queries the live service for current room state

## Daemon

### `bm daemon start`

Start the event-driven daemon for a team.

```bash
bm daemon start [-t <team>] [--mode <mode>] [--port <port>] [--interval <interval>] [--bind <addr>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--mode <mode>` | No | `webhook` or `poll` (default: `webhook`) |
| `--port <port>` | No | HTTP listener port (default: `8484`) |
| `--interval <interval>` | No | Poll interval in seconds for poll mode (default: `60`) |
| `--bind <addr>` | No | Bind address for the HTTP server (default: `0.0.0.0`) |
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
- **Member names** for `bm members show <member>` and `bm chat <member>`
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

## Agent tools (`bm-agent`)

The `bm-agent` binary provides agent-consumed tools that run inside member workspaces. It is separate from the operator-facing `bm` CLI per ADR-0010.

!!! note "Not for operators"
    These commands are designed for automated use by coding agents (e.g., Claude Code hooks), not for direct operator use. Operators can use `bm-agent inbox peek` for debugging.

### `bm-agent inbox write`

Send a message to the loop's inbox. Used by the brain process to send feedback to a coding agent.

```bash
bm-agent inbox write "fix the CI pipeline" [--from <sender>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `<message>` | Yes | Message text (non-empty) |
| `--from <sender>` | No | Sender identity (default: `brain`) |

**Behavior:**

- Appends a JSONL entry to `.ralph/loop-inbox.jsonl` in the workspace root
- Uses `flock` for concurrent write safety
- Requires being inside a BotMinter workspace (`.botminter.workspace` marker)
- Rejects empty or whitespace-only messages

### `bm-agent inbox read`

Read and consume pending messages.

```bash
bm-agent inbox read [--format json|hook]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--format <format>` | No | Output format: `json` or `hook` (default: `hook`) |

**Behavior:**

- Reads all pending messages and truncates the inbox file (best-effort consumption)
- `json` format: outputs a JSON array of message objects
- `hook` format: outputs a JSON object with `additionalContext` key (for Claude Code hooks)
- Empty inbox produces no output in `hook` format, or `[]` in `json` format

### `bm-agent inbox peek`

View pending messages without consuming them.

```bash
bm-agent inbox peek
```

**Behavior:**

- Displays pending messages with timestamp, sender, and content
- Does not modify the inbox file
- Shows "No pending messages." if the inbox is empty

### `bm-agent claude hook post-tool-use`

PostToolUse hook handler for Claude Code. Checks the inbox and returns `additionalContext` if messages are pending.

```bash
bm-agent claude hook post-tool-use
```

**Behavior:**

- Always exits with code 0 — errors are silently swallowed
- If messages are pending: outputs JSON with `additionalContext` and consumes the messages
- If no messages or not in a workspace: outputs nothing
- Designed to be referenced in `.claude/settings.json` as a PostToolUse hook

### `bm-agent loop start`

Start a new Ralph loop via the daemon's HTTP API. Used by the brain process to delegate loop spawning to the daemon supervisor.

```bash
bm-agent loop start "Implement issue #5: add caching" [--member <name>]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `prompt`  | Yes      | The prompt for the Ralph loop |
| `--member`| No       | Member whose workspace to use (defaults to first member) |

**Behavior:**

- Requires `BM_TEAM_NAME` environment variable to be set
- Connects to the running daemon via its HTTP API
- Sends `POST /api/loops/start` with the prompt
- Prints the loop ID to stdout on success
- Exits with code 1 if the daemon is not running or the request fails

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
