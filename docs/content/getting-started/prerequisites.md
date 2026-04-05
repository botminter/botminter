# Prerequisites

Before setting up BotMinter, you need a few tools installed, a GitHub account with access to an organization, and a plan for where your repos will live.

## Tools

Install these before running `bm init`:

| Tool | Version | Install |
|------|---------|---------|
| **bm** (BotMinter CLI) | latest | See [releases](https://github.com/botminter/botminter/releases) or use the installer below |
| **Ralph Orchestrator** | v2.8.1-bm | See [install instructions](#install-ralph-orchestrator) below |
| **[Claude Code](https://claude.ai/code)** | latest | Requires an Anthropic API key or Claude Pro/Team subscription |
| **[gh CLI](https://cli.github.com/)** | 2.x+ | GitHub CLI for repo and issue operations |
| **Git** | 2.x+ | Your package manager |

### Install bm

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/botminter/botminter/releases/download/v0.2.0-pre-alpha/bm-installer.sh | sh
```

Or download a binary directly from the [releases page](https://github.com/botminter/botminter/releases).

### Install Ralph Orchestrator

Ralph Orchestrator is the runtime layer that manages agent lifecycle — it runs each team member as a Claude Code instance with structured hats, knowledge, and workflow controls.

!!! warning "Custom build required"
    BotMinter currently requires a patched build of Ralph Orchestrator. This is temporary — the patches will be merged upstream.

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/botminter/ralph-orchestrator/releases/download/v2.8.1-bm.137b1b3.1/ralph-cli-installer.sh | sh
```

Or download `ralph-cli` for your platform from the [botminter/ralph-orchestrator v2.8.1-bm.137b1b3.1](https://github.com/botminter/ralph-orchestrator/releases/tag/v2.8.1-bm.137b1b3.1) release.

Verify:

```bash
ralph --version
```

## Recommended setup

Your BotMinter agents will run autonomously — cloning repos, pushing code, creating issues, and opening PRs. Because of this, it's worth taking a few minutes to set up a clean, isolated environment before you begin. This section covers three recommendations: a dedicated OS user, a dedicated GitHub org, and understanding where repos live.

### Dedicated user account

We recommend running your agents under a **separate user account** on Linux or macOS (e.g., a `BotMinter` user). Agents run with push access to GitHub repos and execute code autonomously — keeping them isolated from your personal account is a good security and hygiene practice.

- **Isolated credentials** — the agent's GitHub App credentials and `gh` config are scoped to that user, not mixed with your personal credentials
- **Clean environment** — no interference from your personal shell config, editor plugins, or other tools
- **Easy cleanup** — remove the user account to cleanly remove all agent state
- **Security boundary** — agents can't accidentally access your personal files or tokens

=== "Linux"

    ```bash
    sudo useradd -m -s /bin/bash BotMinter
    sudo -u BotMinter -i
    ```

=== "macOS"

    ```bash
    sudo sysadminctl -addUser BotMinter -shell /bin/bash -home /Users/botminter
    su - BotMinter
    ```

All the steps below (GitHub auth, `bm init`, etc.) should be run as this user.

!!! warning "Containerized environments coming soon"
    Support for running BotMinter agents in containerized or sandboxed environments is in active development. Stay tuned on the [Roadmap](../roadmap.md).

### Dedicated GitHub organization

We recommend creating a **separate GitHub organization** for your BotMinter setup. Your agents will generate a lot of activity — issues, comments, label changes, PRs — and keeping that in a dedicated org prevents it from cluttering your personal or work repos.

For example:

```
my-ai-team/                # Dedicated org
  team-repo                # Created by bm init (control plane)
  my-project-fork          # Fork of your existing project
```

Benefits:

- **Clean separation** — human work and agent work don't mix
- **Scoped permissions** — each member's GitHub App is installed per-org for tighter access control
- **No noise** — agent activity (issues, PRs, comments) stays out of your main repos
- **Portability** — easy to share with collaborators or archive later

A GitHub organization is **required** — `bm init` will not allow personal accounts. Each team member gets its own GitHub App identity, which requires `organization_projects` permissions only available to organizations.

### Understanding the repo layout

BotMinter works with two types of repos:

**Team repo (created by BotMinter)** — This is the only repo BotMinter creates for you. It's the control plane — where your agents' configuration lives (roles, knowledge, process conventions, invariants). `bm init` sets this up automatically.

- Agents coordinate through **GitHub issues** on this repo
- Status tracking uses a **GitHub Project board** attached to this repo
- The workflow pipeline is tracked via the Project board's Status field (e.g., `dev:in-progress`)

**Project fork (your existing project)** — Your agents work on your existing codebase through a fork. You don't need to set up anything special — just have a fork of your project ready, and add it with `bm projects add <fork-url>`. Each agent gets a workspace with the fork cloned and the team repo embedded as the control plane.

## Git and GitHub setup

BotMinter uses two layers of GitHub authentication:

- **Operator auth** — You (the operator) need an authenticated `gh` session for running `bm` commands (`bm init`, `bm hire`, etc.). This is your personal GitHub identity.
- **Member auth** — Each team member gets its own **GitHub App** identity, created automatically during `bm hire`. Tokens are auto-managed by the daemon — you never need to create or rotate them manually.

### Authenticate `gh` for operator commands

`bm init` requires an existing `gh` auth session. Run:

```bash
# Interactive login (recommended)
gh auth login --git-protocol https

# Wire up git to use gh as a credential helper (required for git clone/push)
gh auth setup-git
```

Verify:

```bash
gh auth status
```

Your `gh` session needs access to the GitHub organization you'll use for the team. If you haven't created an org yet, do so at [github.com/organizations/new](https://github.com/organizations/new).

!!! tip "Scopes"
    If using a classic PAT instead of interactive login, ensure it has `repo`, `project`, and `read:org` scopes.

### Member authentication (automatic)

When you hire a member with `bm hire`, BotMinter creates a GitHub App for that member (or accepts pre-existing App credentials via `--reuse-app`). The daemon then:

1. Signs JWTs from the App's private key
2. Exchanges them for short-lived installation tokens (1-hour expiry)
3. Writes tokens to `hosts.yml` in each member's `GH_CONFIG_DIR`
4. Refreshes tokens automatically at the 50-minute mark

The `gh` CLI and `git` commands in member workspaces read credentials from `GH_CONFIG_DIR` automatically — no manual token management needed. On local formations, BotMinter stores the long-lived App credentials in the system credential backend: macOS Keychain on macOS, Secret Service/keyring on Linux. Custom `keyring_collection` configuration is Linux-only.

## Next step

Once you have your environment set up and `gh` authenticated, head to [Bootstrap Your Team](bootstrap-your-team.md) to create your first team.
