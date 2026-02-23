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
3. **Profile** — team methodology (e.g., `scrum`, `scrum-compact`, `scrum-compact-telegram`)
4. **GitHub integration** — auto-detects your `GH_TOKEN` or `gh auth` session, validates the token, then lets you browse orgs and select or create a repo
5. **Telegram bot token** — optional, for Human-in-the-Loop notifications (required for `scrum-compact-telegram`, optional for others)
6. **Members** — optionally hire members during init
7. **Projects** — select project repos from the same GitHub org (HTTPS-only)

## What `bm init` does

1. **Detects GitHub auth** — checks `GH_TOKEN` env var, then `gh auth token`; shows masked token for confirmation
2. **Validates token** — calls `gh api user` to verify credentials before proceeding
3. **Creates team directory** — `{workzone}/{team-name}/team/` with git init
4. **Extracts profile** — copies PROCESS.md, CLAUDE.md, knowledge/, invariants/, agent/ from the embedded profile
5. **Hires members** — if specified, extracts member skeletons into `team/{role}-{name}/`
6. **Adds projects** — if specified, creates project directories and updates `botminter.yml`
7. **Creates initial commit** — `git add -A && git commit`
8. **Creates GitHub repo** — runs `gh repo create` and pushes (uses the validated token)
9. **Bootstraps labels** — applies the profile's label scheme; stops with remediation commands on failure
10. **Creates GitHub Project** — creates a v2 Project board with Status field options from the profile
11. **Registers in config** — saves team to `~/.botminter/config.yml` (0600 permissions)

!!! warning "Team name must be unique"
    `bm init` refuses to create a team if the target directory already exists. Choose a different name or delete the existing directory.

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

### 4. Provision workspaces

```bash
bm teams sync
```

This creates member workspaces with the target project clone, `.botminter/` team repo clone, surfaced files (PROMPT.md, CLAUDE.md, ralph.yml), and assembled `.claude/agents/`.

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

| Profile | Description |
|---------|-------------|
| `scrum` | Scrum-style team with pull-based kanban, status labels, conventional commits |
| `scrum-compact` | Single-agent "superman" profile with GitHub comment-based human review |
| `scrum-compact-telegram` | Same as compact but uses Telegram (RObot) for blocking HIL approval gates |

Use `bm profiles describe <name>` for detailed information about roles and labels.

## Related topics

- [Architecture](../concepts/architecture.md) — profile-based generation model
- [Profiles](../concepts/profiles.md) — what profiles contain
- [CLI Reference](../reference/cli.md) — full command documentation
