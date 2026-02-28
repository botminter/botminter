# Getting Started

This guide walks you through setting up your own Claude Code agents, hiring a member, and launching it. Familiarity with Git, GitHub, and command-line tools is assumed.

!!! warning "Pre-Alpha"
    BotMinter is under active development. Commands, configuration format, and behavior may change without notice between releases. See the [Roadmap](../roadmap.md) for current status.

!!! note
    This guide uses the `scrum-compact` profile as an example — a single agent (role: `superman`) that wears multiple hats: product owner, architect, developer, QE, and more. The commands and workflow are the same for any profile — only the profile name and available roles differ. See [Profiles](../concepts/profiles.md) for available profiles.

## Prerequisites

Make sure you've completed the [Prerequisites](index.md) setup — tools, recommended environment, Git and GitHub authentication — before proceeding.

## Step 1: Create a team

Run the interactive wizard:

```bash
bm init
```

The wizard walks you through the full setup:

1. **Workzone directory** — where teams live (default: `~/.botminter/workspaces`)
2. **Team name** — identifier for your team (e.g., `my-team`)
3. **Profile** — team methodology (e.g., `scrum-compact`, `scrum`, `scrum-compact-telegram`)
4. **GitHub integration** — auto-detects your `gh auth` session, validates the token, then lets you browse orgs and select or create a repo interactively
5. **Project board** — select an existing GitHub Project board or create a new one
6. **Telegram bot token** — optional, for Human-in-the-Loop notifications (required for `scrum-compact-telegram`, optional for others)
7. **Members and projects** — optionally hire members and add project fork URLs right away

### What `bm init` does

When the wizard completes, it has:

- **Created (or cloned) a team repo** — the control plane with your profile's process conventions, knowledge structure, and role definitions
- **Bootstrapped GitHub labels** — status labels matching the profile's workflow pipeline (e.g., `status/dev:in-progress`, `status/po:triage`)
- **Created (or selected) a GitHub Project board** — for tracking issue status across roles
- **Registered the team** in your local config (`~/.botminter/config.yml`)
- **Hired members** (if you chose to during the wizard) — extracted member skeletons into the team repo
- **Added projects** (if you chose to during the wizard) — registered project fork URLs in the team config

Config is saved early so that if a GitHub operation fails, the team is still registered and recoverable. If any GitHub operation fails, the wizard stops with actionable error messages showing the exact `gh` commands to run manually.

??? note "What gets created on disk"
    ```
    workzone/
      my-team/
        team/                           # Team repo (control plane, git repo)
          botminter.yml                 # Profile manifest (roles, statuses, views)
          PROCESS.md                    # Issue format, labels, communication protocols
          CLAUDE.md                     # Team-wide agent context
          knowledge/                    # Team-level knowledge files
          invariants/                   # Team-level quality rules
          agent/
            skills/                     # Shared agent skills (gh CLI wrapper)
          skills/                       # Profile-level skills (knowledge-manager, etc.)
          formations/                   # Deployment targets (local, k8s)
          projects/                     # Project-specific knowledge and invariants
          team/                         # Member configurations (populated if you hired during init)
    ```

## Step 2: Hire members and add projects

If you already hired members and added projects during `bm init`, skip to [Step 3](#step-3-provision-workspaces).

### Hire a member

Add a member to the team by specifying a role from the profile:

```bash
bm hire superman
```

This extracts the member skeleton from the embedded profile into the team repo — including its Ralph config, prompts, knowledge, and invariants. With the `scrum-compact` profile, the `superman` role is a single agent that wears all hats (PO, architect, developer, QE).

You can optionally provide a name:

```bash
bm hire superman --name atlas
```

To see what roles are available in your profile:

```bash
bm roles list
```

### Add a project

Register a project fork for your agents to work on:

```bash
bm projects add https://github.com/my-ai-team/my-project-fork
```

This tells BotMinter which codebase your agents will clone and work in. The URL should point to a fork of your project (see [Prerequisites — repo layout](index.md#understanding-the-repo-layout)).

`bm projects add` also creates a `project/<name>` label on the team repo (e.g., `project/my-project-fork`). This label is how agents know which issues belong to which project.

!!! tip "Tag your issues"
    When creating issues on the team repo, make sure to apply the `project/<name>` label so agents can associate the work with the right codebase.

## Step 3: Provision workspaces

Once you have members hired and projects added, provision the workspaces:

```bash
bm teams sync --push
```

This is where the setup becomes real. `bm teams sync` does the following for each hired member:

- **Pushes the team repo** to GitHub (with `--push`) so agents can coordinate via issues
- **Creates a workspace directory** per member × project
- **Clones the project fork** into the workspace
- **Embeds the team repo** as `.botminter/` inside the workspace
- **Surfaces configuration files** — symlinks `PROMPT.md` and `CLAUDE.md` from the team repo, copies `ralph.yml`
- **Assembles `.claude/agents/`** — merges agent definitions from team, project, and member scopes via symlinks

If you've already pushed the team repo, you can run `bm teams sync` without `--push`.

## Step 4: Set up the Project board

Sync the GitHub Project board's status columns with your profile and get instructions for creating role-based views:

```bash
bm projects sync
```

This updates the board's Status field options to match your profile's workflow stages, then prints step-by-step instructions for creating filtered views — one per role — so each agent sees only the statuses relevant to it.

Example output for the `scrum-compact` profile:

```
✓ Status field synced (25 options)

Your GitHub Project board needs role-based views so each role sees
only its relevant statuses. Create one view per role listed below.

Open the board: https://github.com/orgs/my-ai-team/projects/1

For each view:
  1. Click "+" next to the existing view tabs
  2. Choose "Board" layout
  3. Rename the tab to the view name below
  4. Click the filter bar and paste the filter string
  5. Click save
  6. To create the next view, click the tab dropdown → Duplicate view, then repeat from step 3

  View        Filter
  ----        ------
  PO          status:po:triage,po:backlog,po:design-review,po:plan-review,po:ready,po:accept,po:merge,done,error
  Architect   status:arch:design,arch:plan,arch:breakdown,arch:in-progress,arch:sign-off,done,error
  Developer   status:dev:ready,dev:implement,dev:code-review,done,error
  QE          status:qe:test-design,qe:verify,done,error
  Lead        status:lead:design-review,lead:plan-review,lead:breakdown-review,done,error
  Specialist  status:sre:infra-setup,cw:write,cw:review,done,error
```

## Step 5: Launch

Start all members:

```bash
bm start
```

Each member launches as a Claude Code instance orchestrated by Ralph, running in its own workspace with its own hats, knowledge, and workflow.

Check status:

```bash
bm status
```

Inspect the team, its members, or configured projects:

```bash
bm teams show                    # Full team details: members, projects, config
bm members show superman-01      # Member details: role, status, knowledge files
bm projects list                 # List configured projects with fork URLs
```

??? note "Workspace layout after sync and launch"
    ```
    workzone/
      my-team/                                   # Team directory
        team/                                    # Team repo (control plane)
          team/superman-01/                      # Member config
          projects/my-project/                   # Project-specific dirs
        superman-01/                             # Member directory
          my-project/                            # Project fork clone (agent CWD)
            .botminter/                          # Team repo clone
            PROMPT.md → .botminter/...           # Symlinked from team repo
            CLAUDE.md → .botminter/...           # Symlinked from team repo
            ralph.yml                            # Copied from team repo
            .claude/agents/                      # Assembled from team/project/member scopes
    ```

## Shell completions (optional)

Enable tab completions for commands, flags, team names, roles, and more:

```bash
# Bash
echo 'eval "$(bm completions bash)"' >> ~/.bashrc

# Zsh
echo 'eval "$(bm completions zsh)"' >> ~/.zshrc

# Fish
bm completions fish > ~/.config/fish/completions/bm.fish
```

Completions are dynamic — they suggest real values from your configuration (team names, roles, members, profiles, etc.). See the [CLI Reference](../reference/cli.md#shell-completions) for all supported shells.

## Next steps

- Follow [Your First Journey](first-journey.md) to create your first epic, launch your agent, and see the full pipeline in action
- Read [The Agentic Workflow](../workflow.md) to understand the philosophy behind the process
- Read [Architecture](../concepts/architecture.md) to understand the profile-based generation model
- Learn about the [Coordination Model](../concepts/coordination-model.md) and pull-based work discovery
- See [CLI Reference](../reference/cli.md) for all available `bm` commands
