# Getting Started

This guide walks you through creating your first agentic team, hiring a member, and launching it. Familiarity with Git, GitHub, and command-line tools is assumed.

!!! note
    This guide uses the `scrum` profile as an example. The commands and workflow are the same for any profile — only the profile name and available roles differ. See [Profiles](../concepts/profiles.md) for available profiles.

## Prerequisites

Install the following tools before proceeding:

| Tool | Purpose |
|------|---------|
| bm CLI | Team management CLI (`cargo install --path crates/bm`) |
| [Ralph orchestrator](https://github.com/mikeyobrien/ralph-orchestrator) | Agent orchestration runtime |
| [gh CLI](https://cli.github.com/) | GitHub CLI for issue coordination |
| Git | Version control |

## Create a team

Run the interactive wizard:

```bash
bm init
```

The wizard prompts you for:

1. **Workzone directory** — where teams live (default: `~/.botminter/workspaces`)
2. **Team name** — identifier for your team (e.g., `my-team`)
3. **Profile** — team methodology (e.g., `scrum`, `scrum-compact`, `scrum-compact-telegram`)
4. **GitHub integration** — auto-detects your `GH_TOKEN` or `gh auth` session, validates the token, then lets you browse orgs and select or create a repo interactively
5. **Telegram bot token** — optional, for Human-in-the-Loop notifications (required for `scrum-compact-telegram`, optional for others)
6. **Members and projects** — optionally hire members and select project repos from the same org

`bm init` extracts the selected profile into a new team repo, creates the GitHub remote, bootstraps labels and a GitHub Project board, and registers the team in your config. If any GitHub operation fails, the wizard stops with actionable error messages showing the exact `gh` commands to run manually.

??? note "What gets created"
    ```
    workzone/
      my-team/
        team/                           # Team repo (control plane, git repo)
          PROCESS.md                    # Issue format, labels, communication protocols
          CLAUDE.md                     # Team-wide agent context
          knowledge/                    # Team-level knowledge files
          invariants/                   # Team-level quality rules
          agent/
            skills/                     # Shared agent skills (gh CLI wrapper)
          team/                         # Member configurations (empty until hire)
    ```

## Hire a team member

Add a member role to the team:

```bash
bm hire human-assistant
```

This extracts the `human-assistant` member skeleton from the embedded profile into `team/human-assistant/`, including its Ralph config, prompts, knowledge, and invariants.

You can optionally provide a name:

```bash
bm hire architect --name bob
```

## Push to GitHub

If you didn't create a GitHub repo during `bm init`, push the team repo manually — members coordinate through GitHub issues:

```bash
cd ~/workspaces/my-team/team
gh repo create my-org/my-team --private --source=. --push
```

## Provision workspaces

Create workspaces for all hired members:

```bash
bm teams sync
```

This creates a workspace per member × project, cloning the project repo, embedding the team repo as `.botminter/`, and surfacing configuration files (PROMPT.md, CLAUDE.md, ralph.yml).

Use `--push` to push the team repo before syncing:

```bash
bm teams sync --push
```

## Launch

Start all members:

```bash
bm start
```

Check status:

```bash
bm status
```

The workspace layout after sync and start:

```
workzone/
  my-team/                             # Team directory
    team/                              # Team repo (control plane)
      team/human-assistant/            # Member config
    human-assistant/                   # Member workspace
      .botminter/                      # Team repo clone
      PROMPT.md → .botminter/...       # Symlinked from team repo
      CLAUDE.md → .botminter/...       # Symlinked from team repo
      ralph.yml                        # Copied from team repo
```

## Next steps

- Read [The Agentic Workflow](../workflow.md) to see what day-to-day life looks like with a running team
- Read [Architecture](../concepts/architecture.md) to understand the profile-based generation model
- Learn about the [Coordination Model](../concepts/coordination-model.md) and pull-based work discovery
- See [CLI Reference](../reference/cli.md) for all available `bm` commands
- Check the [Roadmap](../roadmap.md) for current milestone status
