# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**botminter** is a Rust CLI (`bm`) for managing GitOps-style agentic teams. Each team member is an independent Ralph orchestrator instance running in its own workspace. Members coordinate through GitHub issues, milestones, and PRs on a shared team repo via the `gh` CLI — no central orchestrator.

The project has completed Milestone 3 (bm CLI). See `specs/master-plan/summary.md` for the full milestone roadmap.

## Commands

### `bm` CLI

```bash
bm init                              # Interactive wizard — create a new team
bm hire <role> [--name <n>] [-t team] # Hire a member into a role
bm projects add <url> [-t team]       # Add a project to the team
bm teams list                         # List registered teams
bm teams sync [--push] [-t team]      # Provision and reconcile workspaces
bm start [-t team]                    # Launch all members (alias: bm up)
bm stop [-t team] [--force]           # Stop all members
bm status [-t team] [-v]              # Status dashboard
bm members list [-t team]             # List hired members
bm roles list [-t team]               # List available roles from profile
bm profiles list                      # List embedded profiles
bm profiles describe <profile>        # Show detailed profile information
```

All commands accepting `-t`/`--team` resolve to the default team when the flag is omitted.

### Development (root Justfile)

```bash
just build    # cargo build -p bm
just test     # cargo test -p bm
just clippy   # cargo clippy -p bm -- -D warnings
```

### Planning workflow

Milestone planning uses reusable prompts in `specs/prompts/`. Feed them to Claude Code via PROMPT.md or direct paste:

- `planning-new.md` — Start planning the next unplanned milestone. Detects which milestone, creates `specs/milestone-*/` with requirements.md and research/.
- `planning-resume.md` — Resume an in-progress milestone's planning session.
- `planning-revisit.md` — Revisit and refine an existing milestone's artifacts.

## Architecture

### Profile-based team generation

Profiles define a team methodology — process conventions, role definitions, member skeletons, knowledge, and invariants. Profiles are embedded in the `bm` binary at compile time via `include_dir`.

| Layer | Location | What lives here | Who changes it |
|-------|----------|-----------------|----------------|
| **Profile** | `profiles/<name>/` | Team process, role definitions, member skeletons, norms | Profile authors |
| **Team repo instance** | e.g., `~/workspaces/my-team/team/` | Project-specific knowledge, hired members, runtime state | Team operators (via `bm` CLI) |

`bm init` runs an interactive wizard that:
1. Detects existing GitHub auth (`GH_TOKEN` env var or `gh auth token`) — prompts only if none found
2. Validates the token via `gh api user` before proceeding
3. Lists the user's GitHub orgs and personal account for interactive selection
4. Offers to create a new repo or select an existing one from the chosen org
5. Bootstraps labels and a GitHub Project (v2) with Status field options from the profile
6. Extracts the profile's content into a new team repo and registers it in `~/.botminter/config.yml`

Project fork URLs are validated as HTTPS-only. If label or project bootstrap fails, the wizard stops with actionable error messages showing the exact `gh` commands to run manually.

### Two-Layer Runtime Model

- **Inner loop:** Each team member is a full Ralph instance with its own hats, memories, PROMPT.md, and workflow.
- **Outer loop:** The team repo is the control plane. GitHub issues on the team repo are the coordination fabric. Members pull work by scanning for status labels matching their role via `gh issue list`.

### Workspace Model

`bm teams sync` provisions workspaces for hired members:

```
workzone/
  my-team/                           # Team directory
    team/                            # Team repo (control plane, git repo)
      team/<member>/                 # Member config (ralph.yml, PROMPT.md, etc.)
    <member>/                        # Member workspace (created by bm teams sync)
      .botminter/                    # Clone of team repo
      PROMPT.md → .botminter/...     # Symlinked from team repo
      CLAUDE.md → .botminter/...     # Symlinked from team repo
      ralph.yml                      # Copied from team repo
      .claude/agents/                # Assembled from three scopes
```

**Surfacing** means symlinking or copying files from the team repo member dir to the workspace root. Runtime files (Ralph memories, scratchpad) stay workspace-local.

### Knowledge & Invariant Scoping (Recursive)

Resolution order — all levels are additive:
1. Team-level: `knowledge/`, `invariants/`
2. Project-level: `projects/<project>/knowledge/`, `projects/<project>/invariants/`
3. Member-level: `team/<member>/knowledge/`, `team/<member>/invariants/`
4. Member+project: `team/<member>/projects/<project>/knowledge/`

### GitHub Coordination

Issues, milestones, and PRs live on the team repo's GitHub. Status transitions use labels following the pattern `status/<role>:<phase>` — the specific roles and phases are profile-defined (e.g., `status/po:triage`, `status/arch:design` in the `scrum` profile). Comments use emoji-attributed format `### <emoji> <role> — <ISO-timestamp>`. Auth uses a shared `GH_TOKEN` stored in `~/.botminter/config.yml` — auto-detected from the environment during `bm init` and passed to all `gh` CLI calls and Ralph instances at runtime.

## Key Directories

| Directory | Purpose |
|-----------|---------|
| `crates/bm/` | Rust binary crate for the `bm` CLI |
| `crates/bm/src/` | Source: cli.rs, config.rs, profile.rs, state.rs, workspace.rs, commands/ |
| `crates/bm/tests/` | Integration tests (full lifecycle, hire, sync, schema guard, multi-team) |
| `docs/` | MkDocs documentation site (`docs/content/` has the markdown, `docs/mkdocs.yml` is the config) |
| `profiles/scrum/` | Scrum profile (PROCESS.md, member skeletons, knowledge, invariants) |
| `profiles/scrum-compact/` | Compact solo profile (single "superman" role) |
| `specs/master-plan/` | Design docs: rough-idea, requirements (26 Q&A), design, plan, research/ |
| `specs/milestone-*/` | Per-milestone specs, requirements, and design docs |
| `specs/prompts/` | Reusable planning prompts (planning-new, planning-resume, planning-revisit) |

## Development Patterns

- **Rust + Cargo workspace:** `crates/bm/` is the main binary crate. Profiles are embedded at compile time via `include_dir`.
- **Specs-first workflow:** Design artifacts in `specs/` are produced before implementation. Each milestone has requirements (Q&A format), design, and plan documents.
- **Incremental milestones:** Each milestone builds on the previous one and is validated with synthetic data before real operational use.
- **Profile reusability:** Changes that apply to a process methodology go in the profile (`profiles/`), not in the generated team repo.
- **Commit convention:** `<type>(<scope>): <subject>` with types `feat|fix|docs|refactor|test|chore`. Include `Ref: #<issue-number>` when applicable. Defined in `profiles/scrum/knowledge/commit-convention.md`.
- **Docs must stay in sync with CLI changes:** The `docs/` directory contains a MkDocs site. When changing CLI behavior (commands, wizard flow, config format), update the corresponding docs in `docs/content/` — especially `getting-started/index.md`, `reference/cli.md`, `how-to/generate-team-repo.md`, and `reference/configuration.md`.
- When embedding a codeblock inside a markdown codeblock, the outer block needs more backticks than the inner block.
- **Invariants:** Development invariants live in `invariants/`. Read them before making changes to the areas they cover. Currently: `e2e-testing.md` (mandatory E2E tests for external API code).
- **E2E tests for API code:** Any code constructing payloads for GitHub's API (`gh` CLI, GraphQL) must have an E2E test in `crates/bm/tests/e2e/` that hits the real API. Run with `cargo test -p bm --features e2e --test e2e -- --test-threads=1`. See `invariants/e2e-testing.md`.

## Generator Repo Runtime

The root of this repo has its own `ralph.yml` (feature-development preset with builder/reviewer hats) and `PROMPT.md` for developing botminter itself via Ralph. These are not part of the CLI output — they configure the development workflow of this repo.

### Launching team members

When launching a team member's Ralph instance from this repo, always use `just dev-launch` (root Justfile) instead of the team repo's `just launch`. This is because developing botminter via Ralph creates a Claude-inside-Claude situation — `just dev-launch` unsets the `CLAUDECODE` env var to allow the nested invocation.
