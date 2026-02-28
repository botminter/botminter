# Prerequisites

Before setting up BotMinter, you need a few tools installed, a GitHub token with the right permissions, and a plan for where your repos will live.

## Tools

Install these before running `bm init`:

| Tool | Version | Install |
|------|---------|---------|
| **[Rust](https://rustup.rs/)** | stable | Required to build the `bm` CLI |
| **bm** (BotMinter CLI) | latest | `cargo install --path crates/bm` |
| **[Ralph orchestrator](https://github.com/mikeyobrien/ralph-orchestrator)** | latest | See the [Ralph repo](https://github.com/mikeyobrien/ralph-orchestrator) for install instructions |
| **[Claude Code](https://claude.ai/code)** | latest | Requires an Anthropic API key or Claude Pro/Team subscription |
| **[gh CLI](https://cli.github.com/)** | 2.x+ | GitHub CLI for repo and issue operations |
| **Git** | 2.x+ | Your package manager |

Ralph orchestrator is the runtime layer that manages agent lifecycle — it runs each team member as a Claude Code instance with structured hats, knowledge, and workflow controls.

## Recommended setup

Your BotMinter agents will run autonomously — cloning repos, pushing code, creating issues, and opening PRs. Because of this, it's worth taking a few minutes to set up a clean, isolated environment before you begin. This section covers three recommendations: a dedicated OS user, a dedicated GitHub org, and understanding where repos live.

### Dedicated user account

We recommend running your agents under a **separate user account** on Linux or macOS (e.g., a `BotMinter` user). Agents run with push access to GitHub repos and execute code autonomously — keeping them isolated from your personal account is a good security and hygiene practice.

- **Isolated credentials** — the agent's GitHub token and `gh` config are scoped to that user, not mixed with your personal credentials
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
- **Scoped permissions** — scope the GitHub token to one org for tighter access control
- **No noise** — agent activity (issues, PRs, comments) stays out of your main repos
- **Portability** — easy to share with collaborators or archive later

If you prefer to keep things under your personal account, that works too — `bm init` lets you choose any org or your personal account interactively.

### Understanding the repo layout

BotMinter works with two types of repos:

**Team repo (created by BotMinter)** — This is the only repo BotMinter creates for you. It's the control plane — where your agents' configuration lives (roles, knowledge, process conventions, invariants). `bm init` sets this up automatically.

- Agents coordinate through **GitHub issues** on this repo
- Status tracking uses a **GitHub Project board** attached to this repo
- The workflow pipeline is tracked via the Project board's Status field (e.g., `dev:in-progress`)

**Project fork (your existing project)** — Your agents work on your existing codebase through a fork. You don't need to set up anything special — just have a fork of your project ready, and add it with `bm projects add <fork-url>`. Each agent gets a workspace with the fork cloned and the team repo embedded as the control plane.

## Git and GitHub setup

Agents use `gh` for GitHub operations (issues, labels, Project boards) and `git` for cloning and pushing repos. Both need to be authenticated with the same token, and both must work non-interactively — agents can't respond to login prompts.

### 1. Create a Personal Access Token

#### Classic PAT (recommended)

Create a [classic PAT](https://github.com/settings/tokens/new) with these scopes:

| Scope | Why it's needed |
|-------|----------------|
| `repo` | Create and manage repos, clone forks, read/write issues and PRs |
| `project` | Create and manage GitHub Projects (v2) for status tracking |
| `read:org` | List your GitHub organizations during `bm init` |

#### Fine-grained PAT (alternative)

If you prefer fine-grained tokens, create one at [Settings > Fine-grained tokens](https://github.com/settings/personal-access-tokens/new) with these permissions:

| Permission | Access | Why |
|-----------|--------|-----|
| Administration | Read & Write | Create repos via `gh repo create` |
| Contents | Read & Write | Clone, create, and push repos |
| Issues | Read & Write | Create labels, read/write issues for coordination |
| Pull requests | Read & Write | Open and manage PRs |
| Projects | Admin | Create and configure GitHub Project boards |
| Metadata | Read | Access repository and organization metadata |

!!! tip
    If you're using an org, make sure the token has access to that org. For fine-grained tokens, set the **resource owner** to the org.

!!! note "Fine-grained PAT limitation"
    Fine-grained tokens cannot list your GitHub organizations automatically. During `bm init`, the org selection step will only show your personal account — you'll need to type your org name manually when prompted. Classic PATs don't have this limitation.

### 2. Authenticate `gh` and `git`

Two things need authentication:

- **`bm` and `gh` commands** — `bm init` detects your token automatically from the `GH_TOKEN` environment variable or `gh auth token`, and stores it in BotMinter's own config (`~/.botminter/config.yml`). All subsequent `gh` calls use this stored token. If no token is detected, `bm init` will prompt you to paste one interactively.
- **`git clone` and `git push`** — During `bm teams sync`, plain `git` commands clone project forks. These need `gh` configured as a credential helper so `git` can authenticate with the same token.

Save your token to a file and run:

```bash
# Authenticate gh and set HTTPS as the Git protocol
gh auth login --with-token --git-protocol https < gh-token.txt

# Wire up git to use gh as a credential helper (required for git clone/push)
gh auth setup-git
```

Verify everything works:

```bash
gh auth status
```

You can delete `gh-token.txt` after login — the token is stored in `gh`'s config.

`bm init` validates the token before proceeding — if it can't authenticate, the wizard will tell you.

## Next step

Once you have your environment set up and token configured, head to [Getting Started](index.md) to create your first team.
