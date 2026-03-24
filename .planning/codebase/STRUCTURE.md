# Codebase Structure

**Analysis Date:** 2026-03-10

## Directory Layout

```
botminter/
├── crates/bm/                  # Main Rust binary crate
│   ├── src/
│   │   ├── main.rs             # Entry point + CLI dispatch
│   │   ├── lib.rs              # Module declarations
│   │   ├── cli.rs              # Clap CLI definition
│   │   ├── commands/           # One file per subcommand group
│   │   │   ├── mod.rs
│   │   │   ├── init.rs         # bm init (wizard + non-interactive)
│   │   │   ├── start.rs        # bm start / bm up
│   │   │   ├── stop.rs         # bm stop
│   │   │   ├── status.rs       # bm status
│   │   │   ├── hire.rs         # bm hire
│   │   │   ├── chat.rs         # bm chat
│   │   │   ├── minty.rs        # bm minty
│   │   │   ├── teams.rs        # bm teams {list,show,sync}
│   │   │   ├── members.rs      # bm members {list,show}
│   │   │   ├── roles.rs        # bm roles list
│   │   │   ├── profiles.rs     # bm profiles {list,describe}
│   │   │   ├── profiles_init.rs# bm profiles init
│   │   │   ├── projects.rs     # bm projects {list,show,add,sync}
│   │   │   ├── knowledge.rs    # bm knowledge {list,show}
│   │   │   ├── bridge.rs       # bm bridge {start,stop,status,identity,room}
│   │   │   ├── daemon.rs       # bm daemon {start,stop,status} + event loop
│   │   │   └── completions.rs  # bm completions
│   │   ├── config.rs           # ~/.botminter/config.yml management
│   │   ├── profile.rs          # Profile parsing, extraction, schema
│   │   ├── workspace.rs        # Workspace provisioning + surfacing
│   │   ├── bridge.rs           # Bridge abstraction + credential store
│   │   ├── chat.rs             # Meta-prompt builder
│   │   ├── formation.rs        # Formation config (local/k8s)
│   │   ├── topology.rs         # Runtime topology (endpoints)
│   │   ├── state.rs            # Runtime state (PIDs)
│   │   ├── session.rs          # Claude/Ralph session launching
│   │   ├── agent_tags.rs       # Agent-specific content filtering
│   │   └── completions.rs      # Dynamic shell completion logic
│   ├── tests/
│   │   ├── integration.rs      # Integration tests (114K, filesystem-based)
│   │   ├── cli_parsing.rs      # CLI arg parsing tests (41K)
│   │   ├── conformance.rs      # Bridge conformance tests (13K)
│   │   ├── bridge_sync.rs      # Bridge sync tests (11K)
│   │   ├── profile_roundtrip.rs# Profile extraction roundtrip (1.8K)
│   │   ├── README.md           # Test documentation
│   │   └── e2e/                # E2E tests (real GitHub)
│   │       ├── main.rs         # libtest-mimic custom harness
│   │       ├── helpers.rs      # Test utilities
│   │       ├── github.rs       # GithubSuite shared-repo pattern
│   │       ├── isolated.rs     # Isolated test scenarios
│   │       ├── telegram.rs     # Telegram bridge e2e
│   │       ├── stub-ralph.sh   # Stub ralph binary for e2e
│   │       └── scenarios/
│   │           ├── mod.rs
│   │           └── operator_journey.rs  # Full operator journey test
│   └── Cargo.toml              # Crate manifest
├── profiles/                    # Embedded team profiles
│   ├── scrum/                   # Full scrum profile (multi-role)
│   └── scrum-compact/           # Compact solo profile (single "superman" role)
│       ├── PROCESS.md           # Process conventions
│       ├── context.md           # Role context (becomes AGENTS.md)
│       ├── .schema/v1.yml       # Schema version marker
│       ├── formations/          # Deployment formations
│       │   ├── local/formation.yml
│       │   └── k8s/             # K8s formation + manager config
│       ├── invariants/          # Constitutional constraints
│       ├── knowledge/           # Shared knowledge docs
│       ├── skills/              # Team-level skills
│       ├── coding-agent/        # Coding-agent-specific files
│       │   ├── agents/          # Agent configs
│       │   ├── skills/          # Agent skills (gh, board-scanner, etc.)
│       │   └── context.md       # Agent-specific context
│       └── ralph-prompts/       # Ralph Orchestrator prompt templates
├── invariants/                  # Project-level constitutional constraints
├── knowledge/                   # Project-level knowledge documents
├── docs/                        # MkDocs documentation site
│   ├── mkdocs.yml               # MkDocs configuration
│   ├── content/                 # Markdown source files
│   ├── overrides/               # Theme overrides
│   └── site/                    # Built static site (generated)
├── minty/                       # Minty assistant config
│   ├── config.yml               # Minty session config
│   └── prompt.md                # Minty system prompt
├── .planning/                   # Planning artifacts (GSD workflow)
│   ├── adrs/                    # Architecture Decision Records
│   ├── specs/                   # Formal specifications
│   ├── phases/                  # Phase execution plans
│   ├── milestones/              # Milestone definitions
│   ├── research/                # Research documents
│   ├── debug/                   # Debug reports
│   └── codebase/                # Codebase analysis (this file)
├── .claude/                     # Claude Code development config
│   ├── agents/                  # Agent definitions
│   └── skills/                  # Development skills
├── Cargo.toml                   # Workspace manifest
├── Cargo.lock                   # Dependency lock
├── Justfile                     # Task runner recipes
├── AGENTS.md                    # Project instructions for Claude
├── ralph.yml                    # Ralph config for developing botminter
├── PROMPT.md                    # -> specs/milestones/.../PROMPT.md
├── README.md                    # Project readme
└── RELEASE_NOTES.md             # Release notes
```

## Directory Purposes

**`crates/bm/src/`:**
- Purpose: All Rust source code for the `bm` CLI binary
- Contains: Domain modules at root level, subcommand handlers in `commands/`
- Key files: `profile.rs` (85K, largest), `workspace.rs` (68K), `bridge.rs` (43K)

**`crates/bm/src/commands/`:**
- Purpose: One-to-one mapping from CLI subcommands to handler functions
- Contains: Each file exports `run()` or named functions matching subcommand variants
- Key files: `init.rs` (60K, most complex), `daemon.rs` (39K), `start.rs` (26K)

**`crates/bm/tests/`:**
- Purpose: Integration and E2E tests (unit tests are inline in source modules)
- Contains: Filesystem-based integration tests, CLI parsing tests, bridge conformance, e2e scenarios
- Key files: `integration.rs` (114K), `cli_parsing.rs` (41K)

**`profiles/`:**
- Purpose: Team methodology templates embedded into the binary at compile time
- Contains: Two profiles (`scrum`, `scrum-compact`), each with process docs, roles, knowledge, invariants, formations, skills
- Key pattern: Files may contain `+agent:NAME` tags for coding-agent-specific content

**`invariants/`:**
- Purpose: Hard constraints that must be satisfied by all code changes
- Contains: Rules about CLI idempotency, e2e coverage, test isolation, flaky tests, profile updates
- Key files: `cli-idempotency.md`, `e2e-scenario-coverage.md`, `no-hardcoded-profiles.md`

**`knowledge/`:**
- Purpose: Reference documents for development workflows and tooling context
- Contains: E2E testing patterns, Ralph Orchestrator internals, Claude Code skill development
- Key files: `e2e-testing-patterns.md`, `nested-claude-code-process-safety.md`

**`docs/`:**
- Purpose: MkDocs documentation site for end users
- Contains: Getting started guides, concept explanations, CLI reference, how-to guides
- Key files: `docs/mkdocs.yml`, `docs/content/reference/cli.md`

**`minty/`:**
- Purpose: Configuration for the Minty interactive assistant feature
- Contains: Claude Code session config and system prompt
- Key files: `minty/config.yml`, `minty/prompt.md`

## Key File Locations

**Entry Points:**
- `crates/bm/src/main.rs`: Binary entry point, CLI dispatch
- `crates/bm/tests/e2e/main.rs`: E2E test harness entry (custom `libtest-mimic` main)

**Configuration:**
- `Cargo.toml`: Workspace manifest (`members = ["crates/*"]`)
- `crates/bm/Cargo.toml`: Crate manifest with dependencies and feature flags
- `Justfile`: Task runner (build, test, clippy, docs, release)
- `docs/mkdocs.yml`: Documentation site config

**Core Logic (by size/importance):**
- `crates/bm/src/profile.rs`: Profile parsing, embedded extraction, schema validation (85K)
- `crates/bm/src/workspace.rs`: Workspace creation, submodule management, file surfacing (68K)
- `crates/bm/src/bridge.rs`: Bridge abstraction, credential storage, lifecycle (43K)
- `crates/bm/src/commands/init.rs`: Team initialization wizard (60K)
- `crates/bm/src/commands/daemon.rs`: Event-driven daemon with webhook/poll modes (39K)
- `crates/bm/src/commands/start.rs`: Member launch orchestration (26K)
- `crates/bm/src/completions.rs`: Dynamic shell completions (20K)
- `crates/bm/src/chat.rs`: Meta-prompt assembly for chat sessions (19K)
- `crates/bm/src/agent_tags.rs`: Agent-specific content filtering (16K)

**Testing:**
- `crates/bm/tests/integration.rs`: Main integration test suite (114K)
- `crates/bm/tests/cli_parsing.rs`: CLI argument parsing tests (41K)
- `crates/bm/tests/conformance.rs`: Bridge conformance tests (13K)
- `crates/bm/tests/e2e/scenarios/operator_journey.rs`: Full operator journey E2E

## Naming Conventions

**Files:**
- Source modules: `snake_case.rs` (e.g., `agent_tags.rs`, `profile_roundtrip.rs`)
- Commands: one file per subcommand group, named after the subcommand (e.g., `init.rs`, `start.rs`, `bridge.rs`)
- Profile content: `kebab-case.md` for knowledge/invariants (e.g., `commit-convention.md`, `cli-idempotency.md`)

**Directories:**
- Rust convention: `snake_case` for module dirs (e.g., `commands/`)
- Profile convention: `kebab-case` for profile content dirs (e.g., `coding-agent/`, `ralph-prompts/`)

**Functions:**
- Command handlers: `pub fn run(...)` or named functions like `list()`, `show()`, `sync()`
- Domain functions: `pub fn load(...)`, `pub fn save(...)`, `pub fn resolve_...()`

**Structs:**
- PascalCase: `BotminterConfig`, `TeamEntry`, `FormationConfig`, `WorkspaceRepoParams`
- Serde-derived with rename attributes for external format compatibility

## Where to Add New Code

**New CLI Subcommand:**
1. Add variant to `Command` enum in `crates/bm/src/cli.rs`
2. Create handler file in `crates/bm/src/commands/{name}.rs`
3. Add `pub mod {name};` to `crates/bm/src/commands/mod.rs`
4. Add dispatch arm in `crates/bm/src/main.rs`
5. Add integration tests in `crates/bm/tests/integration.rs`
6. Update docs in `docs/content/reference/cli.md`

**New Domain Module:**
1. Create `crates/bm/src/{name}.rs`
2. Add `pub mod {name};` to `crates/bm/src/lib.rs`
3. Import in command handlers that need it

**New Profile:**
1. Create directory under `profiles/{name}/`
2. Must include `PROCESS.md`, `.schema/v1.yml`, role definitions
3. Follow existing profile structure (see `profiles/scrum-compact/` as minimal example)
4. Binary must be recompiled (profiles are embedded at compile time via `include_dir!`)

**New Integration Test:**
- Add `#[test]` function in `crates/bm/tests/integration.rs`
- Use `tempfile::tempdir()` for filesystem isolation

**New E2E Test Scenario:**
- Add scenario file in `crates/bm/tests/e2e/scenarios/`
- Register in `crates/bm/tests/e2e/scenarios/mod.rs`
- Wire into harness in `crates/bm/tests/e2e/main.rs`
- Requires `TESTS_GH_TOKEN` + `TESTS_GH_ORG` environment variables

**New Invariant:**
- Add markdown file to `invariants/` (project-level) or `profiles/*/invariants/` (profile-level)
- Follow format in `knowledge/invariant-format.md`

**New Knowledge Document:**
- Add markdown file to `knowledge/` (project-level) or `profiles/*/knowledge/` (profile-level)

## Special Directories

**`profiles/`:**
- Purpose: Team methodology templates embedded into binary
- Generated: No (authored manually)
- Committed: Yes
- Note: Compiled into binary via `include_dir!()` macro; changes require rebuild

**`docs/site/`:**
- Purpose: Built MkDocs static site
- Generated: Yes (by `just docs-build`)
- Committed: Partially (appears in repo)

**`target/`:**
- Purpose: Cargo build artifacts
- Generated: Yes
- Committed: No (gitignored)

**`.planning/`:**
- Purpose: GSD workflow artifacts, ADRs, specs, research
- Generated: Partially (some generated by planning tools)
- Committed: Yes

**`.ralph/`:**
- Purpose: Ralph Orchestrator runtime state for developing botminter itself
- Generated: Yes (runtime)
- Committed: Partially

**`~/.botminter/` (runtime, not in repo):**
- Purpose: Global CLI state — config, runtime state, daemon configs, logs
- Key files: `config.yml`, `state.json`, `daemon-{team}.json`, `logs/`
- Generated: Yes (by `bm init` and other commands)
- Committed: No (user home directory)

---

*Structure analysis: 2026-03-10*
