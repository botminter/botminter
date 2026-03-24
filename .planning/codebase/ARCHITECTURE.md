# Architecture

**Analysis Date:** 2026-03-10

## Pattern Overview

**Overall:** CLI-driven GitOps control plane with profile-based code generation

**Key Characteristics:**
- Single Rust binary (`bm`) that orchestrates team lifecycle via filesystem + GitHub API
- Two-layer runtime: outer loop (CLI manages members) and inner loop (each member is a Ralph Orchestrator instance)
- Profiles are embedded at compile time and extracted to disk as team repos
- GitHub issues/labels/projects serve as the coordination fabric between members
- All state lives in files: `~/.botminter/config.yml`, `~/.botminter/state.json`, `topology.json`

## Layers

**CLI Layer (Presentation):**
- Purpose: Parse commands, dispatch to handler functions
- Location: `crates/bm/src/cli.rs`, `crates/bm/src/main.rs`
- Contains: Clap derive structs (`Cli`, `Command`, subcommand enums), match dispatch in `main()`
- Depends on: Commands layer
- Used by: End user via `bm` binary

**Commands Layer (Application Logic):**
- Purpose: Implement each CLI subcommand as a standalone function
- Location: `crates/bm/src/commands/` (one file per subcommand group)
- Contains: `init.rs` (60K, wizard logic), `start.rs` (26K, member launch), `daemon.rs` (39K, event loop), `chat.rs` (19K, interactive sessions), `bridge.rs` (18K), `teams.rs` (14K), `hire.rs` (8K), `stop.rs` (5K), `status.rs` (13K), `knowledge.rs` (10K), `members.rs` (8K), `projects.rs` (10K), `profiles.rs` (3K), `profiles_init.rs` (18K), `roles.rs` (1K), `minty.rs` (7K), `completions.rs` (1K)
- Depends on: Domain layer (config, profile, workspace, bridge, state, topology, formation, session)
- Used by: CLI layer

**Domain Layer (Core Abstractions):**
- Purpose: Model the domain concepts and provide reusable operations
- Location: `crates/bm/src/` (top-level modules)
- Contains:
  - `config.rs` — Global config (`~/.botminter/config.yml`) load/save/resolve
  - `profile.rs` (85K) — Profile parsing, embedded extraction, schema validation, agent tag filtering
  - `workspace.rs` (68K) — Workspace provisioning, submodule setup, file surfacing
  - `bridge.rs` (43K) — Bridge plugin abstraction, credential storage, lifecycle management
  - `chat.rs` (19K) — Meta-prompt building for interactive sessions
  - `formation.rs` — Formation config (local vs k8s deployment)
  - `topology.rs` — Runtime topology (where members are running)
  - `state.rs` — Runtime state (PIDs, process lifecycle)
  - `session.rs` — Claude Code / Ralph session launching
  - `agent_tags.rs` (16K) — Coding-agent-specific content filtering
  - `completions.rs` (20K) — Dynamic shell completions
- Depends on: Filesystem, `gh` CLI, system keyring
- Used by: Commands layer

**External Systems:**
- Purpose: Side effects and integrations
- Location: Accessed via `std::process::Command` calls to `gh`, `git`, `claude`, `ralph`, `just`
- Contains: GitHub API (via `gh` CLI), system keyring (via `keyring` crate), filesystem operations
- Depends on: Nothing internal
- Used by: Domain layer, Commands layer

## Data Flow

**Team Initialization (`bm init`):**

1. User runs `bm init` (interactive) or `bm init --non-interactive ...`
2. `commands/init.rs` validates inputs, detects GitHub auth (`GH_TOKEN` or `gh auth token`)
3. `profile.rs` extracts embedded profile to `{workzone}/{team}/team/` on disk
4. `commands/init.rs` bootstraps GitHub: creates/selects repo, pushes team repo, creates labels + Project board
5. `config.rs` saves team entry to `~/.botminter/config.yml`

**Member Launch (`bm start`):**

1. `commands/start.rs` loads config, resolves team, validates schema version
2. `formation.rs` resolves formation type (local, k8s)
3. `workspace.rs` discovers member directories under `{workzone}/{team}/team/members/`
4. For each member: spawns `ralph run` as background process via `std::process::Command`
5. `state.rs` records PIDs in `~/.botminter/state.json`
6. `topology.rs` writes `topology.json` with endpoint info (local PID or k8s pod)

**Workspace Sync (`bm teams sync`):**

1. `commands/teams.rs` loads config, resolves team
2. `workspace.rs` creates workspace repo per member: git init, add team repo as submodule, add project forks as submodules
3. Surfaces files: copies `PROMPT.md`, `AGENTS.md`, `ralph.yml` from team repo member dir to workspace root
4. Creates symlinks for `.claude/agents/` and `.claude/skills/` pointing into team submodule
5. Writes `.botminter.workspace` marker file with agent identification tags

**State Management:**
- Global config: `~/.botminter/config.yml` (YAML, teams + credentials + workzone path)
- Runtime state: `~/.botminter/state.json` (JSON, member PIDs + workspaces)
- Topology: `{workzone}/{team}/topology.json` (JSON, member endpoints)
- Bridge state: `{workzone}/{team}/bridge-state.json` (JSON, identity mappings)
- Daemon state: `~/.botminter/daemon-{team}.json` + `daemon-{team}-poll.json`

## Key Abstractions

**Profile:**
- Purpose: Defines a team methodology (roles, process, knowledge, invariants, formations)
- Examples: `profiles/scrum/`, `profiles/scrum-compact/`
- Pattern: Embedded at compile time via `include_dir!()`, extracted to disk by `profile.rs`
- Key struct: `ProfileManifest` in `crates/bm/src/profile.rs`

**Team Entry:**
- Purpose: Registered team with its config, credentials, and path
- Examples: Stored in `~/.botminter/config.yml` under `teams[]`
- Pattern: `TeamEntry` struct in `crates/bm/src/config.rs` — name, path, profile, github_repo, credentials
- Resolution: `config::resolve_team()` takes optional `-t` flag and falls back to `default_team`

**Workspace:**
- Purpose: A member's working directory with submodules to team repo and project forks
- Examples: `{workzone}/{team}/{member}/`
- Pattern: Git repo with `team/` submodule + `projects/{name}/` submodules + surfaced config files
- Key function: `workspace::create_workspace_repo()` in `crates/bm/src/workspace.rs`

**Bridge:**
- Purpose: Chat platform integration (Telegram, future Rocket.Chat)
- Examples: Bridge manifests in `profiles/*/bridges/`
- Pattern: `CredentialStore` trait with `LocalCredentialStore` (keyring) and `InMemoryCredentialStore` (tests)
- Key trait: `CredentialStore` in `crates/bm/src/bridge.rs`

**Formation:**
- Purpose: Deployment model (local processes vs k8s pods)
- Examples: `profiles/*/formations/local/`, `profiles/*/formations/k8s/`
- Pattern: `FormationConfig` struct loaded from `formation.yml`, resolution defaults to "local"
- Key struct: `FormationConfig` in `crates/bm/src/formation.rs`

**Agent Tags:**
- Purpose: Filter profile content for specific coding agents (Claude Code, Gemini CLI)
- Examples: `<!-- +agent:claude-code -->` blocks in markdown, `# +agent:gemini-cli` in YAML
- Pattern: Line-based filter in `crates/bm/src/agent_tags.rs` strips/includes content per agent

## Entry Points

**`main()` — CLI binary:**
- Location: `crates/bm/src/main.rs`
- Triggers: User runs `bm <subcommand>`
- Responsibilities: Parse CLI args via Clap, dispatch to `commands::*` functions

**`commands/daemon.rs::run_daemon()` — Event loop:**
- Location: `crates/bm/src/commands/daemon.rs`
- Triggers: `bm daemon-run` (internal, spawned by `bm daemon start`)
- Responsibilities: Listen for GitHub webhooks or poll for events, dispatch to member Ralph sessions

**`session.rs::interactive_claude_session()` — Chat sessions:**
- Location: `crates/bm/src/session.rs`
- Triggers: `bm chat <member>`, `bm minty`
- Responsibilities: Build meta-prompt, launch `claude` binary with system prompt, block until exit

## Error Handling

**Strategy:** `anyhow::Result<()>` throughout, with `bail!()` for expected errors and `.context()` for wrapping

**Patterns:**
- Commands return `Result<()>` — errors propagate to `main()` and print with full context chain
- `bail!()` with actionable messages: include the failing entity name and suggest next steps (e.g., "Run `bm init` first")
- Idempotency checks before mutations: `gh repo view` before `gh repo create`, file existence checks before writes
- Atomic writes for state files: write to `.tmp`, then `fs::rename()` (see `state.rs`, `topology.rs`)

## Cross-Cutting Concerns

**Logging:** `eprintln!()` for warnings, `println!()` for user-facing output. No structured logging framework.

**Validation:** Schema version checks via `profile::check_schema_version()` before operations. Team name validation (no `/` or spaces). URL validation (HTTPS-only for project forks).

**Authentication:** GitHub token resolved from `GH_TOKEN` env var or `gh auth token` during `bm init`. Stored in `~/.botminter/config.yml` (0600 permissions). Bridge tokens stored in system keyring via `CredentialStore` trait. Config file permissions checked on load with warning if not 0600.

**Security:** Config files written with 0600 permissions. Topology files written with 0600 permissions. `CLAUDECODE` env var explicitly removed when spawning nested Claude Code instances.

---

*Architecture analysis: 2026-03-10*
