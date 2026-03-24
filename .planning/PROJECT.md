# BotMinter

## What This Is

Running one coding agent is easy. Running a team of them is challenging. BotMinter is a CLI that brings conventions to running a team of coding agents. Your process, knowledge, and constraints live in a Git repo, and every agent picks them up automatically. Built for Claude Code today, with a coding-agent-agnostic architecture to support Gemini CLI, Codex, and more.

## The Problem

Most tools focus on *how to run agents* — spawning, orchestrating, lifecycle. BotMinter solves a different problem: *how your agents should work, what they know, and how you stay in the loop.*

When you run several agents across multiple projects, four gaps show up fast:

- **Reuse**: How do you apply the same conventions to all your agents without copying them into every config?
- **Customization**: How do you keep shared defaults but override just one thing for a specific project or agent — without forking the entire config?
- **Propagation**: When you update a convention, how does it reach every agent — without you touching each one?
- **Visibility**: When your agents are working, how do you know what each one decided and why — without reading terminal logs?

## Core Value

**Profiles** — Git-backed convention packages you pick once and customize from there. Like Helm for Kubernetes or Rails for web, a profile ships opinionated defaults: roles, process, knowledge scoping, quality gates, and workspace layout. Push a file to the repo at the right scope, every relevant agent picks it up. Agents coordinate through GitHub issues, so every decision is traceable on a board — not buried in a terminal session.

**Layered Knowledge Scoping** is the primary differentiator. Knowledge and constraints resolve at four levels — all additive: team-wide -> project-wide -> member-wide -> member+project. Write a convention once at the right scope, and it reaches exactly the agents that need it.

## Requirements

### Validated

- v0.05: Profile-based team generation with embedded profiles (scrum, scrum-compact; Telegram available as bridge)
- v0.05: Interactive `bm init` wizard with GitHub auth, org selection, repo creation, label/project bootstrap
- v0.05: `bm hire` for adding members to roles with profile-driven skeletons
- v0.05: `bm teams sync` for provisioning workspaces
- v0.05: `bm start/stop/status` for member lifecycle management
- v0.05: `bm projects add/list/show` for project management
- v0.05: Event-driven daemon (webhook and poll modes)
- v0.05: Profile schema with `botminter.yml` + `.schema/`
- v0.05: Shell completions with dynamic values
- v0.03: GitHub Projects v2 status tracking with profile-defined status field options
- v0.01: Two-layer runtime model: inner loop (Ralph instance per member) + outer loop (team repo as control plane)
- v0.01: Recursive knowledge/invariant scoping: team -> project -> member -> member+project
- v0.02: Board scanner pattern with poll-based and event-triggered execution models
- v0.03: Supervised mode with human gates at design review, plan review, and acceptance
- v0.06: Agent tag filter library for coding-agent-agnostic file processing (CAA-01)
- v0.06: `CodingAgentDef` data model with profile/team override resolution (CAA-02)
- v0.06: Profile restructuring: `coding-agent/` directory, `context.md` with inline agent tags (CAA-03)
- v0.06: Unified-to-agent-specific extraction: agent tag filtering + context.md rename (CAA-04)
- v0.06: Workspace parameterization: all hardcoded agent strings replaced with resolved config (CAA-05)
- v0.06: `bm profiles init` for extracting embedded profiles to `~/.config/botminter/profiles/` (PROF-01..05)
- v0.06: Disk-based profile API: all operations read from `~/.config/botminter/profiles/` (PROF-03)
- v0.06: Auto-prompt pattern: `ensure_profiles_initialized()` detects missing profiles (PROF-04)
- v0.06: Workspace repository model with GitHub-hosted repos and git submodules (WRKS-01..06)
- v0.06: Board-scanner skill migration and profile directory restructuring (SKIL-01..05)
- v0.06: Ralph prompt shipping: `ralph-prompts/` directory in all profiles (SKIL-03)
- v0.06: Status-workflow skill: composable skill for status transitions (SKIL-04)
- v0.06: Team Manager role: profile definition, skeleton, minimal statuses (TMGR-01..03)
- v0.06: `bm chat` for interactive coding agent sessions with `build_meta_prompt()` (CHAT-01..03)
- v0.06: `bm minty` launcher with Minty persona and 4 composable skills (MNTY-01..04)
- v0.06: Documentation updates for all 6 sprints (CAA-06, PROF-05, WRKS-06, SKIL-05, CHAT-03, MNTY-04)

### Active

#### Current Milestone: v0.07 Team Bridge

**Goal:** Decouple communication into a pluggable "bridge" abstraction, ship a local Slack-like reference implementation (Rocket.Chat), migrate Telegram into the same abstraction, and establish ADRs + Knative-style specs for extensible interfaces.

**Target features:**
- Bridge abstraction: pluggable communication backend defined by shell script lifecycle (start, stop, health, configure) with stdout config
- Bridge CLI: `bm bridge start/stop/status` with optional auto-start from `bm start`
- Rocket.Chat bridge: reference implementation for local Slack-like experience with per-agent identity
- Telegram bridge: wrap existing Telegram support into the bridge abstraction
- Ralph Orchestrator robot abstraction: upstream contribution to make Ralph's robot backend pluggable
- Agent identity: each team member gets their own bot user on the bridge
- ADRs + specs: Architecture Decision Records and Knative-style specs for extensible interfaces (bridge spec first)

### Out of Scope

- Multi-coding-agent support beyond Claude Code — architecture is pluggable, only Claude Code implemented (A5)
- Migration paths or backwards compatibility — Alpha policy, operators re-create from scratch (A16)
- Full Team + First Story milestone — deferred pending operator experience improvements
- Eval/Confidence System — deferred pending multi-member practical experience

## Context

BotMinter is pre-alpha. Six milestones complete (v0.01 through v0.06). The full workflow works end-to-end (`bm init` -> hire -> sync -> start -> agents work issues -> you review at gates), and v0.06 added coding-agent-agnostic architecture, disk-based profiles, workspace repos with submodules, composable skills, `bm chat` for interactive sessions, and `bm minty` as an interactive assistant.

Two profiles ship today: `scrum` (multi-agent, one per role) and `scrum-compact` (solo agent, all hats). Telegram is available as an optional bridge on any profile. The `bm` CLI is a Rust binary (`crates/bm/`) with profiles embedded at compile time via `include_dir` and extractable to disk for customization.

21,267 lines of Rust. 471 tests (327 unit + 49 cli_parsing + 95 integration). Prior planning artifacts in `specs/` (PDD format).

Communication is currently tied to Telegram via Ralph Orchestrator's hardcoded robot backend. Ralph has a robot abstraction concept but no pluggable implementation — Telegram is the only option. The v0.07 milestone decouples communication at both layers: a robot abstraction in Ralph (upstream contribution) and a bridge abstraction in BotMinter that manages service lifecycle and agent identity.

Architectural practice is evolving: v0.07 introduces ADRs for decision tracking and Knative-style specs for extensible interfaces. The bridge plugin contract will be the first formally specified extension point.

## Constraints

- **Tech stack**: Rust + Cargo workspace, profiles embedded via `include_dir`, `gh` CLI for GitHub operations
- **Runtime dependency**: Ralph Orchestrator (open source, local checkout at `/opt/workspace/ralph-orchestrator`)
- **Alpha policy**: Breaking changes expected, no migration, no backwards compatibility
- **Naming**: Always "Ralph Orchestrator" for the product, "coding-agent-agnostic" not "LLM-agnostic"

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| GitHub issues + Projects v2 for coordination, no central orchestrator | Emergent coordination from shared process and status conventions | Good |
| Each member is a full Ralph instance in isolated workspace | Avoids single-agent-many-masks; preserves independent memories/context | Good |
| Config-driven coding agent mapping with profile/team override | Pluggable architecture without premature multi-agent support | Good |
| Inline agent tags (`+agent:NAME/-agent`) for file sections | Unified files with inline markers vs separate file variants per agent | Good |
| Profiles on disk at `~/.config/botminter/profiles/` | Editable/customizable without rebuilding binary; separated from runtime data | Good |
| Workspace repos as GitHub-hosted git repos with submodules | Clean separation, no nested AGENTS.md confusion, multi-project support | Good |
| Skills as composable building blocks for roles and Minty | Shared delivery mechanism for team-level and BotMinter-level interactions | Good |
| Closure DI pattern for test isolation (session.rs, E2E) | Avoids env::set_var race conditions; explicit dependency injection | Good |
| libtest-mimic custom harness for E2E tests | Mandatory CLI args (--gh-token, --gh-org) without env var fragility | Good |
| GithubSuite abstraction for E2E test repo sharing | Reduces API rate limit consumption from 19 to ~11 TempRepo creations | Good |
| Two-layer profile model: embedded for bootstrap, disk for runtime | Ensures CLI always works (embedded) while supporting customization (disk) | Good |

---
*Last updated: 2026-03-08 after v0.07 milestone started*
