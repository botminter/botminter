# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**botminter** is a Rust CLI (`bm`) for managing GitOps-style agentic teams. Each team member is an independent Ralph orchestrator instance running in its own workspace. Members coordinate through GitHub issues, milestones, and PRs on a shared team repo via the `gh` CLI — no central orchestrator.

The project has completed Milestone 6 (Minty and Friends). See `.planning/ROADMAP.md` for the current milestone roadmap.

## Commands

### `bm` CLI

```bash
bm init                              # Interactive wizard — create a new team
bm init --non-interactive ...        # Scripted/CI mode (requires --profile, --team-name, --org, --repo; optional --bridge)
bm hire <role> [--name <n>] [-t team] # Hire a member into a role
bm chat <member> [-t team] [--hat h]  # Interactive session with a member
bm projects add <url> [-t team]       # Add a project to the team
bm projects list [-t team]            # List configured projects
bm projects show <project> [-t team]  # Show project details
bm teams list                         # List registered teams
bm teams show [<name>] [-t team]      # Show detailed team info
bm teams sync [--repos] [--bridge] [--all|-a] [-v] [-t team] # Provision and reconcile workspaces
bm start [-t team]                    # Launch all members (alias: bm up)
bm stop [-t team] [--force]           # Stop all members
bm status [-t team] [-v]              # Status dashboard
bm members list [-t team]             # List hired members
bm members show <member> [-t team]    # Show member details
bm roles list [-t team]               # List available roles from profile
bm profiles list                      # List embedded profiles
bm profiles describe <profile>        # Show detailed profile information
```

All commands accepting `-t`/`--team` resolve to the default team when the flag is omitted.

### Development (root Justfile)

```bash
just build        # cargo build -p bm
just unit         # Unit tests only (no GitHub token needed)
just conformance  # Bridge conformance tests only
just e2e          # E2E tests only (requires TESTS_GH_TOKEN + TESTS_GH_ORG)
just e2e-step     # Progressive E2E — one case at a time
just e2e-reset    # Clean up progressive E2E state
just test         # All tests: unit + conformance + e2e
just clippy       # cargo clippy -p bm -- -D warnings
just docs-serve   # Live-reload MkDocs dev server at localhost:8000
just docs-build   # Build static docs site
just release version notes_file  # Tag + GitHub release
```

### Running a single test

```bash
# Run a single unit/integration test by name
cargo test -p bm <test_name>

# Run a single E2E scenario (libtest-mimic filter)
cargo test -p bm --features e2e --test e2e -- --gh-token "$TESTS_GH_TOKEN" --gh-org "$TESTS_GH_ORG" <scenario_name> --test-threads=1
```

### E2E test harness

E2E tests use `libtest-mimic` with a custom `main()` — not standard `#[test]` macros. Key differences:
- Standard flags like `--nocapture` don't work — use `eprintln!()` instead (stderr is always visible)
- Custom args `--gh-token` and `--gh-org` are required
- The `e2e` feature gate must be enabled (`--features e2e`)
- Tests run against real GitHub — they create/delete repos and projects

### Planning workflow

Milestone planning uses the GSD (Get Shit Done) workflow. Planning artifacts live in `.planning/` with phase-based execution plans, research, and context documents. See `.planning/ROADMAP.md` for milestone structure and `.planning/STATE.md` for current position.

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
      members/<member>/              # Member config (ralph.yml, PROMPT.md, etc.)
    <member>/                        # Workspace repo (created by bm teams sync)
      team/                          # Submodule → team repo
      projects/
        <project>/                   # Submodule → project fork
      PROMPT.md                      # Copied from team/members/<member>/
      AGENTS.md                      # Copied from team/members/<member>/
      ralph.yml                      # Copied from team/members/<member>/
      .claude/agents/                # Symlinks into team/ submodule paths
      .botminter.workspace           # Workspace marker file
```

**Surfacing** means copying or symlinking files from the team repo member dir to the workspace root. Runtime files (Ralph memories, scratchpad) stay workspace-local.

### Knowledge & Invariant Scoping (Recursive)

Resolution order — all levels are additive:
1. Team-level: `knowledge/`, `invariants/`
2. Project-level: `projects/<project>/knowledge/`, `projects/<project>/invariants/`
3. Member-level: `members/<member>/knowledge/`, `members/<member>/invariants/`
4. Member+project: `members/<member>/projects/<project>/knowledge/`

### GitHub Coordination

Issues, milestones, and PRs live on the team repo's GitHub. Status transitions use labels following the pattern `status/<role>:<phase>` — the specific roles and phases are profile-defined (e.g., `status/po:triage`, `status/arch:design` in the `scrum` profile). Comments use emoji-attributed format `### <emoji> <role> — <ISO-timestamp>`. Auth uses a shared `GH_TOKEN` stored in `~/.botminter/config.yml` — auto-detected from the environment during `bm init` and passed to all `gh` CLI calls and Ralph instances at runtime.

## Key Directories

| Directory | Purpose |
|-----------|---------|
| `crates/bm/` | Rust binary crate for the `bm` CLI |
| `crates/bm/src/` | Source modules (see table below) |
| `crates/bm/tests/` | Integration tests (full lifecycle, hire, sync, schema guard, multi-team) |
| `docs/` | MkDocs documentation site (`docs/content/` has the markdown, `docs/mkdocs.yml` is the config) |
| `profiles/scrum/` | Scrum profile (PROCESS.md, member skeletons, knowledge, invariants) |
| `profiles/scrum-compact/` | Compact solo profile (single "superman" role) |
| `crates/bm/tests/e2e/` | E2E tests against real GitHub (init, sync, bridge lifecycle) |
| `invariants/` | Constitutional constraints — hard requirements, not suggestions |
| `.planning/adrs/` | Architecture Decision Records (MADR 4.0.0 format) |
| `.planning/specs/` | Formal specifications for external contracts and plugin interfaces |

### Source modules (`crates/bm/src/`)

| Module | Purpose |
|--------|---------|
| `cli.rs` | Clap CLI definition and subcommand dispatch |
| `config.rs` | `~/.botminter/config.yml` read/write |
| `profile.rs` | Embedded profile extraction and validation |
| `formation.rs` | Orchestrates `bm init` — the multi-step wizard |
| `topology.rs` | Resolves team/member/project paths and relationships |
| `workspace.rs` | Workspace provisioning and file surfacing |
| `bridge.rs` | Bridge plugin abstraction (Telegram, Rocket.Chat, Matrix/Tuwunel) |
| `session.rs` | Ralph session management (start/stop/status) |
| `state.rs` | Runtime state persistence (`state.json`) |
| `agent_tags.rs` | Agent identification tags in workspace markers |
| `commands/` | One file per CLI subcommand (init, hire, start, stop, etc.) |

## Development Patterns

- **Rust + Cargo workspace:** `crates/bm/` is the main binary crate. Profiles are embedded at compile time via `include_dir`.
- **Specs-first workflow:** Design artifacts in `.planning/specs/` define external contracts before implementation. Architectural decisions are recorded as ADRs in `.planning/adrs/`.
- **Incremental milestones:** Each milestone builds on the previous one and is validated with synthetic data before real operational use.
- **Profile reusability:** Changes that apply to a process methodology go in the profile (`profiles/`), not in the generated team repo.
- **Commit convention:** `<type>(<scope>): <subject>` with types `feat|fix|docs|refactor|test|chore`. Include `Ref: #<issue-number>` when applicable. Defined in `profiles/scrum/knowledge/commit-convention.md`.
- **Docs must stay in sync with CLI changes:** The `docs/` directory contains a MkDocs site. When changing CLI behavior (commands, wizard flow, config format), update the corresponding docs in `docs/content/` — especially `getting-started/index.md`, `reference/cli.md`, `how-to/generate-team-repo.md`, and `reference/configuration.md`.
- When embedding a codeblock inside a markdown codeblock, the outer block needs more backticks than the inner block.
- **Invariants are constitutional.** All files in `invariants/` are hard constraints that MUST be satisfied — they are not suggestions. Read them before making changes and review compliance after implementation. Violations are treated as bugs.
- **CLI idempotency:** All state-mutating commands (`init`, `teams sync`, `bridge identity add`, `bridge room create`) MUST be idempotent. Running the same command twice must produce the same end state without errors. Check for existing state before creating (e.g., `gh repo view` before `gh repo create`). See `invariants/cli-idempotency.md`.
- **E2E test coverage per profile variation:** Each meaningful profile × bridge combination needs a happy path e2e test in `crates/bm/tests/e2e/`. The happy path must exercise the full operator journey (init → hire → configure → sync → verify). Integration tests cover variations and edge cases. Current variations: scrum-compact (no bridge), scrum-compact + telegram, scrum (no bridge), scrum + telegram.
- **User-scenario TDD:** When adding features, write the e2e test first (what the user should experience), then fix code to make it green. Plans that decompose into implementation tasks without user-journey tests will miss display/integration gaps that only show up in UAT.
- **Keyring prerequisites (Linux):** The `keyring` crate requires a Secret Service provider with an initialized collection. On desktop Linux this happens via PAM; on headless/su/SSH access it may not. Error messages must distinguish "no daemon" from "daemon running but collection missing." See `.planning/debug/keyring-report.md` and `.planning/todos/pending/2026-03-09-improve-local-formation-keyring-ux.md`.

## Generator Repo Runtime

The root of this repo has its own `ralph.yml` (feature-development preset with builder/reviewer hats) and `PROMPT.md` for developing botminter itself via Ralph. These are not part of the CLI output — they configure the development workflow of this repo.

### Launching team members

When launching a team member's Ralph instance from this repo, always use `just dev-launch` (root Justfile) instead of the team repo's `just launch`. This is because developing botminter via Ralph creates a Claude-inside-Claude situation — `just dev-launch` unsets the `CLAUDECODE` env var to allow the nested invocation.

### DANGER: Nested Claude Code & Process Safety

Commands like `bm chat`, `bm minty`, and `bm start` launch Claude Code. When you are running inside Ralph (i.e., you ARE a Claude Code instance managed by Ralph), this creates a Claude-inside-Claude situation.

- **CLAUDECODE env var:** You MUST unset `CLAUDECODE` before launching Claude Code for testing. Use `CLAUDECODE= bm chat ...` or `env -u CLAUDECODE bm chat ...`. The nested instance will refuse to start otherwise.
- **NEVER kill Ralph:** You MUST NOT run `kill`, `pkill`, `killall`, or any signal-sending command against Ralph or its parent processes. If you need to stop a process you spawned for testing, kill it ONLY by the specific PID you received — never by name, never by pattern.
- **No `bm stop` against yourself:** Do NOT run `bm stop` during implementation — it terminates Ralph (your own orchestrator).

## Alpha Policy

- **Breaking changes are expected.** During Alpha, every change is a breaking change. No migration paths, no backwards compatibility shims, no upgrade tooling. Operators re-create teams/workspaces from scratch when the model changes.

## Naming Conventions

- Always **"Ralph Orchestrator"** when referring to the product/project — never just "Ralph" in product context. Casual references to `ralph.yml` or Ralph instances within a team are fine, but the product name is "Ralph Orchestrator."
- Use **"coding-agent-agnostic"** (not "LLM-agnostic") when talking about abstracting away Claude Code / Gemini CLI specifics. The LLM layer is already abstracted by Ralph Orchestrator; the coding agent layer is what BotMinter needs to abstract.

## Progressive Interactive E2E Testing

When stepping through e2e tests progressively with the user (`just e2e-step`), follow this workflow strictly:

1. **Run a progressive step** — execute `just e2e-step SUITE=<name>`.
2. **Collect evidence** — run evidence commands (state file, GitHub checks, filesystem) and write the commands + raw output to `target/e2e-evidence/step-NN.txt`.
3. **Report** — tell the user the evidence file path. Wait for them to review and confirm.
4. **If step passed** — wait for confirmation before running the next step.
5. **If step failed** — describe the bug and propose a fix. **Do NOT apply the fix.** Wait for the user to confirm the fix before implementing it.
6. **After fix confirmed + applied** — re-run the same step, collect evidence, wait for confirmation.

Key rules:
- Never apply a fix without user confirmation.
- Always write evidence to a file, not inline in the conversation.
- Evidence must include the raw commands that generated the output.
- **Evidence must prove the side effects actually happened**, not just that the test returned exit 0. Examples:
  - `bm hire` → `ls` the members dir to show the member was created
  - `bridge identity add` → `secret-tool search` to show the token is in the keyring
  - `bm start` → `ps` to show the process is running; check `.ralph-stub-env` for correct env vars
  - `bm stop` → `ps` to show the process is gone; check `state.json` for empty members
  - `teams sync` → `ls` the workspace dir to show files were created
  - Label creation → `gh label list` to show the label exists on GitHub
- Evidence files live in `crates/bm/target/e2e-evidence/`. Always run `just` from the project root.

## Ralph Orchestrator
- Ralph Orchestrator project is an open source project and a dependency for BotMinter.
- The GitHub repo is https://github.com/mikeyobrien/ralph-orchestrator/
- There is a local checked out version under /opt/workspace/ralph-orchestrator
- The checked out version has a local commit that we created to support setting a custom Telegram URL which is needed to run a Telegram mock server in e2e tests

## GUIARDRAILS / INVARIANTS / MUST COMPLY
- You MUST use just test to validate any changes at least once before the task is done.
- You MUST fix any failures even if they're unrelated to your changes. You CAN present the user the situation before you fix such irrelevant failures.
- You MUST focus on improving the quality of the code and you SHOULD leave the code better than you found it.
- You SHOULD suggest any improvement or enhancements to the code or to AGENTS.md whenever an improvement presents itself.
