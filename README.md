<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/readme-banner-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="assets/readme-banner-light.png">
    <img alt="BotMinter" src="assets/readme-banner-light.png" width="400">
  </picture>
</p>

Running one coding agent is easy. Running a team of them is challenging.

BotMinter is a CLI that brings conventions to running a team of coding agents. Your process, knowledge, and constraints live in a Git repo, and every agent picks them up automatically. Built for Claude Code today, with architecture to support Gemini CLI, Codex, and more.

> [!NOTE]
> **What's next:** The current version was the experiment - it validated the thesis. Now BotMinter is generalizing into something bigger. The next version will look fundamentally different. [Follow along](https://github.com/botminter/botminter/discussions/6).

## The Problem

Most tools in this space focus on how to run agents - spawning them, orchestrating multi-agent pipelines, managing lifecycle. BotMinter solves a different problem: how your agents should work, what they know, and how you stay in the loop.

Because when you run several agents across multiple projects, the gaps show up fast. Same conventions copied everywhere, changes applied one agent at a time, and the only way to see what your agents decided is to read through their terminal sessions.

- **Reuse**: How do you apply the same conventions to all your agents without copying them into every config?
- **Customization**: How do you keep shared defaults but override just one thing for a specific project or agent - without forking the entire config?
- **Propagation**: When you update a convention, how does it reach every agent - without you touching each one?
- **Visibility**: When your agents are working, how do you know what each one decided and why - without reading terminal logs?

BotMinter answers all four with a batteries-included approach: **profiles** - Git-backed convention packages you pick once and customize from there. Push a file to the repo at the right scope, every relevant agent picks it up. Agents coordinate through GitHub issues, so every decision is traceable on a board - not buried in a terminal session.

## Profiles

Like Helm for Kubernetes or Rails for web, a profile ships opinionated defaults for coding agents. It defines:

- **Roles & Process** - who does what, how work flows between them, what quality gates apply
- **Knowledge & Constraints** - four-level scoping system (team → project → member → member+project)
- **Communication** - how agents surface decisions to you for approval
- **Runtime & Workspace** - where agents execute, how directories are laid out

You pick a profile when you run `bm init`. It stamps out a team repo you own and customize from there.

### What ships today

All profiles share the same knowledge scoping, constraint system, workspace layout, and local sandboxed runtime. They differ in two dimensions:

| | Roles | Communication |
|---|---|---|
| **`scrum-compact`** | Single agent - PO, architect, dev, QE | GitHub Issues + Matrix (default) |

> The `scrum` profile (multi-role teams with separate agents per role) is in development and will ship in a future release.

Everything is customizable after init - add roles, redefine pipeline phases, change gate criteria, or extend the workspace layout.

### Layered Knowledge Scoping

This is BotMinter's primary differentiator. Knowledge and constraints resolve at four levels - all additive:

```
team-wide              All your agents, all projects
  └─ project-wide      All your agents on this project
      └─ member-wide   This agent, all projects
          └─ member+project   This agent, this project
```

**Example:** You decide all your agents should use `pnpm`, never `npm`. You create `knowledge/use-pnpm.md` at the team level. Every agent on every project sees it on next launch. Later, your backend project has a specific database constraint - you add it at the project level. Only agents working on that project pick it up. No copy-pasting between agents. No repeating yourself.

**What this looks like on disk:**

Knowledge files are information agents should know. Invariants are constraints agents must not violate. Both follow the same scoping rules.

```
my-team/                                    # Team repo
  knowledge/                                # Team-wide - all agents see this
    use-pnpm.md
    no-raw-sql.md
  invariants/                               # Team-wide constraints
    pr-coverage-80.md
  projects/backend/
    knowledge/                              # Project-wide - only backend agents
      db-migration-rules.md
  members/dev-01/
    knowledge/                              # Member-wide - only dev-01
      azure-deploy-notes.md
    projects/backend/
      knowledge/                            # Member+project - dev-01 on backend only
        backend-quirks.md
```

## Quick Start

### Prerequisites

[Claude Code](https://claude.ai/code), [Ralph Orchestrator](#install-ralph-orchestrator), [gh CLI](https://cli.github.com/), and Git. A GitHub token with `repo`, `project`, and `read:org` scopes. See the full [Prerequisites](https://botminter.github.io/botminter/getting-started/prerequisites/) guide.

### Install and run

```bash
# Install bm (Linux x86_64 - see releases for other platforms)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/botminter/botminter/releases/download/v0.2.0-pre-alpha/bm-installer.sh | sh

bm init                                      # Interactive wizard - team name, profile, GitHub org/repo
bm hire superman                             # Add an agent (the all-in-one role in scrum-compact)
bm projects add https://github.com/my-org/my-project
bm teams sync --repos                        # Provision workspaces

bm start                                     # Launch agents
bm status                                    # Check status
```

### Install Ralph Orchestrator

> [!WARNING]
> This release requires a patched build of Ralph Orchestrator. This is temporary — the patches will be merged upstream.

Download `ralph-cli` for your platform from the [botminter/ralph-orchestrator v2.8.1-bm.137b1b3.1](https://github.com/botminter/ralph-orchestrator/releases/tag/v2.8.1-bm.137b1b3.1) release, or use the installer:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/botminter/ralph-orchestrator/releases/download/v2.8.1-bm.137b1b3.1/ralph-cli-installer.sh | sh
```

See [Your First Journey](https://botminter.github.io/botminter/getting-started/first-journey/) for a complete walkthrough.

## Core Commands

```bash
bm init                              # Interactive wizard - create a new team
bm hire <role> [--name <n>] [-t team] # Hire an agent into a role
bm projects add <url> [-t team]       # Add a project
bm teams list                         # List registered teams
bm teams sync [--repos] [-t team]     # Provision and reconcile workspaces
bm start [-t team]                    # Launch all agents
bm stop [-t team] [--force]           # Stop all agents
bm status [-t team] [-v]              # Status dashboard
bm members list [-t team]             # List agents
bm roles list [-t team]               # List available roles
bm profiles list                      # List available profiles
bm profiles describe <profile>        # Show detailed profile information
```

See the full [CLI Reference](https://botminter.github.io/botminter/reference/cli/) for all commands.

## Documentation

Full documentation at **[botminter.github.io/botminter](https://botminter.github.io/botminter/)**:

- [Prerequisites](https://botminter.github.io/botminter/getting-started/prerequisites/) - Tools, GitHub auth, recommended setup
- [Getting Started](https://botminter.github.io/botminter/getting-started/) - Step-by-step team creation
- [Your First Journey](https://botminter.github.io/botminter/getting-started/first-journey/) - End-to-end walkthrough
- [Profiles](https://botminter.github.io/botminter/concepts/profiles/) - Available profiles and customization
- [FAQ](https://botminter.github.io/botminter/faq/) - Common questions

## Development

```bash
just build    # cargo build -p bm
just test     # cargo test -p bm
just clippy   # cargo clippy -p bm -- -D warnings
```

## License

Apache License 2.0 - see [LICENSE](LICENSE).
