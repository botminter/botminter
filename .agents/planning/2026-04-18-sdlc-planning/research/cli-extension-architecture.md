# R-08: CLI Extension Architecture

## Context

The `bm` CLI is a monolithic Rust/Clap 4 binary with no plugin or extension mechanism. All commands are compile-time Clap derive variants dispatched through a single `match` in `main.rs`. Adding a command today requires modifying `cli.rs`, `main.rs`, and a new command file.

The SDLC planning redesign introduces profile-specific CLI commands (`bm plan`, `bm verify`) that should only exist when the active profile declares them. This requires a CLI extension mechanism.

## Research: How Other CLIs Do Extensions

**git/cargo/kubectl** — PATH-based discovery. Any `git-foo` binary on PATH becomes `git foo`. No registration needed. Simple but no help text integration, no arg validation, shell scripts as extension language.

**Clap built-in** — `#[command(external_subcommand)]` catches unrecognized subcommands as `Vec<OsString>`. Clap's builder API (`Command::new().subcommand()`) supports runtime subcommand registration. The two can be combined: derive for static commands, builder for dynamic. The `build_cli_with_completions()` pattern in the existing `bm` codebase already rebuilds the CLI tree programmatically — this is a natural extension point.

**cargo `[alias]`** — Config-driven aliases in `.cargo/config.toml` map short names to command strings. Hierarchical config scoping (project overrides workspace overrides global). Simple but limited to command aliasing.

**nushell** — Full plugin protocol with signature negotiation. Over-engineered for our needs.

## Existing `bm` Architecture (Key Findings)

- `bm chat <member> [--hat h]` already does 90% of what `bm plan` needs: resolve team, prepare meta-prompt with hat context, launch coding agent in workspace.
- `chat::prepare_chat_session()` is the domain-layer function that builds the meta-prompt and resolves the formation.
- The `botminter.yml` manifest already declares roles, statuses, labels, views, coding agents, bridges. Adding an `extensions` field is natural.
- `build_cli_with_completions()` already injects dynamic data (team names, member names) into the CLI tree at runtime — extending this to inject subcommands from the manifest follows the same pattern.
- There is no `bm team` (singular). Only `bm teams` (plural) for list/show/sync.

## Decision: Manifest-Driven Extensions (Option B)

### How It Works

1. **Profile declares extensions** in `botminter.yml`:
   ```yaml
   extensions:
     - name: plan
       description: "Start a collaborative planning session"
       member: engineer
       hat: lead_plan-create
       args:
         - name: idea
           positional: true
           required: false
           description: "Rough idea to plan"
         - name: epic
           long: epic
           type: int
           description: "Epic issue number to load as input"
     - name: verify
       description: "Verify acceptance criteria for completed work"
       member: engineer
       hat: qe_verify
       args:
         - name: work-item
           positional: true
           required: true
           description: "Issue number to verify"
   ```

2. **At startup, `bm` discovers the active team** from the workspace marker (`.botminter.workspace` in CWD or parent dirs) or the default team in config. Reads the team's `botminter.yml` manifest.

3. **Extensions are registered as top-level Clap subcommands** via the builder API before `get_matches()` is called. This gives proper help text, arg validation, and shell completions.

4. **Dispatch is generic:** when an extension subcommand matches, `bm` resolves the member + hat from the extension definition, calls `chat::prepare_chat_session()` with those params, and launches the coding agent session — identical to `bm chat <member> --hat <hat>` but with profile-defined defaults.

### Chicken-and-Egg Resolution

To register extension subcommands, the manifest must be read before CLI parsing. But the active team depends on the workspace context. Resolution:

1. Detect workspace from CWD (walk up to find `.botminter.workspace` marker)
2. If found, read the team's `botminter.yml`
3. If extensions exist, inject them as builder subcommands
4. Parse CLI args with the augmented command tree
5. If no workspace detected, skip extension injection — static commands only

This mirrors cargo's approach (reads `.cargo/config.toml` before parsing subcommands).

### What Extensions Can Express

All current use cases are "start a chat session with a specific member and hat." The manifest format captures:

- **Which member** to start the session with (resolved to the actual hired member name via role)
- **Which hat** to activate
- **Arguments** that are forwarded to the session (positional args become the initial prompt, flags become context)
- **Description** for help text

This is intentionally limited. Extensions are session launchers, not arbitrary CLI commands. If future extensions need arbitrary behavior, the system can be upgraded to a plugin trait.

### Implementation Scope

Changes to `bm` binary (3 files + 1 new):

1. **`cli.rs`** — Add `#[command(external_subcommand)] External(Vec<OsString>)` variant to the `Command` enum.
2. **`main.rs`** — Add dispatch arm for `External` that resolves the extension definition and calls `chat::prepare_chat_session()`.
3. **`profile/manifest.rs`** — Add `extensions: Vec<Extension>` to `ProfileManifest` struct. Define `Extension` and `ExtensionArg` structs.
4. **`commands/extension.rs`** (new) — Generic extension dispatch: resolve member from role, validate hat exists, build initial prompt from args, call `chat::prepare_chat_session()`.

For completions: extend `build_cli_with_completions()` to also inject extension subcommands.

### Why Not Shell Scripts or PATH-Based

- Shell scripts lose Clap integration (no help text, no completions, no arg validation)
- PATH-based discovery (`bm-plan`) requires managing binaries outside the profile, breaking portability
- Manifest-driven extensions are portable with the profile — `bm init --profile X` gives you the commands
