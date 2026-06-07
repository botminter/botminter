# Generate a Team Repo

This guide covers creating a new team using the `bm init` interactive wizard, including post-generation setup.

## Create a team

Run the interactive wizard:

```bash
bm init
```

The wizard will prompt you for:

1. **Workzone directory** — where teams live (default: `~/.botminter/workspaces`)
2. **Team name** — identifier for your team (e.g., `my-team`)
3. **Profile** — team methodology (e.g., `agentic-sdlc-minimal`)
4. **Bridge** — communication bridge (e.g., Matrix via Tuwunel, the default) if the profile supports one
5. **GitHub integration** — auto-detects your `gh auth` session, validates it, then lets you browse orgs and select or create a repo
6. **Project board** — select an existing GitHub Project board or create a new one
7. **Members** — optionally hire members during init (new repos only)
8. **Projects** — select project repos from the same GitHub org (HTTPS-only, new repos only)

!!! note "Existing repos"
    When selecting an existing repo, the wizard skips member hiring and project addition — the repo already has its own content. Use `bm hire` and `bm projects add` after init to modify the team.

## What `bm init` does

**For new repos:**

1. **Detects GitHub auth** — auto-detects your `gh auth` session; shows masked token for confirmation
2. **Validates credentials** — calls `gh api user` to verify authentication before proceeding
3. **Creates team directory** — `{workzone}/{team-name}/team/` with git init
4. **Extracts profile** — copies PROCESS.md, context.md (renamed to the agent's context file), knowledge/, invariants/, coding-agent/ from the profile on disk, filtering agent-specific content
5. **Hires members** — if specified, extracts member skeletons into `members/{role}-{name}/`
6. **Adds projects** — if specified, creates project directories and updates `botminter.yml`
7. **Creates initial commit** — `git add -A && git commit`
8. **Creates GitHub repo** — runs `gh repo create` and pushes (uses the validated token)
9. **Registers in config** — saves team to `~/.botminter/config.yml` (0600 permissions)
10. **Bootstraps labels** — applies the profile's label scheme; stops with remediation commands on failure
11. **Creates/syncs GitHub Project** — creates a new board or syncs Status field options on an existing one

**For existing repos:**

1. **Detects and validates GitHub auth** — same as above
2. **Clones the existing repo** — into `{workzone}/{team-name}/team/`
3. **Registers in config** — saves team to `~/.botminter/config.yml` (0600 permissions)
4. **Bootstraps labels** — idempotent (uses `--force`)
5. **Creates/syncs GitHub Project** — creates a new board or syncs Status field options on an existing one

!!! warning "Team name must be unique"
    `bm init` refuses to create a team if the target directory already exists. Choose a different name or delete the existing directory.

## Non-interactive mode

For CI pipelines or scripted setup:

```bash
bm init --non-interactive \
  --profile agentic-sdlc-minimal \
  --team-name my-team \
  --org my-org \
  --repo my-team-repo \
  --project new
```

This runs the full init flow without prompts -- creates the GitHub repo, bootstraps labels, creates a Project board, and registers the team. Requires `gh auth login` or `GH_TOKEN` in the environment.

All required parameters must be provided as flags. See the [CLI reference](../reference/cli.md#non-interactive-mode) for the full parameter list.

## Post-generation setup

### 1. Push to GitHub (if not done during init)

Members coordinate through GitHub issues, so the repo needs a GitHub remote:

```bash
cd ~/workspaces/my-team/team
gh repo create my-org/my-team --private --source=. --push
```

### 2. Hire team members

```bash
bm hire architect --name bob
bm hire human-assistant --name alice
```

See [Manage Members](manage-members.md) for details.

### 3. Add projects

```bash
bm projects add https://github.com/org/my-project.git
```

!!! note
    Project URLs must be HTTPS. SSH URLs are not supported.

### 4. Launch members

```bash
bm start
```

This creates ephemeral session workspaces for each member from shared repository clones, assembles context files (PROMPT.md, CLAUDE.md, ralph.yml), and launches agents.

### 5. Add project-specific knowledge

Populate `projects/<project>/knowledge/` with domain-specific context:

```bash
cd ~/workspaces/my-team/team
cp ~/docs/architecture.md projects/my-project/knowledge/
git add projects/my-project/knowledge/architecture.md
git commit -m "docs: add project architecture knowledge"
```

## Available profiles

Use `bm profiles list` to see all available profiles:

| Profile | Description | Status |
|---------|-------------|--------|
| `agentic-sdlc-minimal` | Minimal agentic SDLC with three roles (engineer, chief-of-staff, sentinel) | Available |
| `scrum` | Multi-role team with pull-based kanban, status labels, conventional commits | In development |

`agentic-sdlc-minimal` supports communication bridges: Matrix via Tuwunel (default), Telegram (experimental), and Rocket.Chat (experimental). Select one during `bm init` or use `--bridge <name>` in non-interactive mode. See the [Bridge Setup Guide](bridge-setup.md) for details.

Use `bm profiles describe <name>` for detailed information about roles and labels.

## Related topics

- [Architecture](../concepts/architecture.md) — profile-based generation model
- [Profiles](../concepts/profiles.md) — what profiles contain
- [CLI Reference](../reference/cli.md) — full command documentation
