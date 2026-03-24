# Phase 9: Profile Integration & Cleanup - Research

**Researched:** 2026-03-08
**Domain:** Profile bridge integration, init wizard extension, teams sync bridge provisioning, credential management
**Confidence:** HIGH

## Summary

Phase 9 integrates bridge selection and provisioning into the full operator journey: `bm init` -> `bm hire` -> `bm teams sync` -> `bm start`. All foundation code exists from Phase 8 -- the `bridge.rs` module handles manifest parsing, state management, recipe invocation, and credential resolution. Phase 9 wires these into the profile system (`botminter.yml` declares supported bridges), the init wizard (bridge selection step), the hire command (optional external bridge token prompt), and teams sync (bridge provisioning and `ralph.yml` RObot section generation).

The codebase is well-structured for this integration. The init wizard uses `cliclack` for interactive prompts (not `dialoguer` as the milestone research suggested). The `ProfileManifest` struct in `profile.rs` currently has no `bridges` field -- this needs to be added. The `Credentials` struct in `config.rs` has a single `telegram_bot_token` field which needs to evolve into a formation-aware credential storage abstraction. The `scrum-compact-telegram` profile audit reveals it is nearly identical to `scrum-compact` -- the only meaningful differences are: (1) `RObot.enabled: true` with timeout/checkin config vs `RObot.enabled: false`, (2) GitHub-comment-based HIL docs in PROCESS.md and communication-protocols.md vs the older Telegram-based HIL docs. The `scrum-compact` profile is the newer, more evolved version -- all unique content worth preserving already exists in `scrum-compact`.

**Primary recommendation:** Extend `ProfileManifest` with a `bridges` field, add bridge selection to init wizard after profile selection, add `--bridge` flag to `bm init --non-interactive`, redesign `bm teams sync` flags, wire bridge provisioning into sync, generate `ralph.yml` RObot section per-member, and remove `scrum-compact-telegram`.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Two bridge categories: Managed (auto-provision) vs External (operator-supplied tokens)
- Per-member identity for both types
- External bridge tokens collected during `bm hire` (prompted, optional -- hire succeeds without)
- Members without bridge credentials flagged; operator can add later via `bm bridge identity add`
- `bm teams sync` skips ralph.yml RObot section generation for members missing credentials
- Formation-aware secret storage: design abstraction in Phase 9, implement local keyring backend
- Bridge selection in init wizard immediately after profile selection
- Wizard lists bridges from profile's supported bridges plus "No bridge" option
- Init only records bridge selection in team config -- does not start the bridge
- `bm init --non-interactive` accepts `--bridge <name>` flag; omitting means no bridge
- `--bridge` is optional for non-interactive mode
- Current `--push` flag replaced with `--repos`, `--bridge`, `--all`/`-a` composable flags
- `bm teams sync` -- local workspace assembly only (default)
- `bm teams sync --repos` -- also push/sync git repositories (replaces `--push`)
- `bm teams sync --bridge` -- also provision bridge identities and rooms
- `bm teams sync --all`/`-a` -- equivalent to `--repos --bridge`
- Bridge provisioning during `--bridge` is idempotent
- `botminter.yml` schema explicitly declares supported bridges (not just directory-based)
- `bridges/` directory in profile contains bridge implementations
- Schema declaration is source of truth; directory provides implementation files
- Audit `scrum-compact-telegram` for unique content before deletion
- Migrate any unique knowledge/invariants/skills to `scrum-compact`
- Delete profile directory and update all references
- Documentation in MkDocs site (`docs/content/`)

### Claude's Discretion
- Exact `botminter.yml` schema shape for the `bridges` declaration
- Internal organization of formation-aware secret storage abstraction
- MkDocs page structure and navigation for bridge docs
- How to handle `--push` flag deprecation (given Alpha policy)
- Test organization for new sync flag behavior

### Deferred Ideas (OUT OF SCOPE)
- K8s formation secret storage backend
- Encrypted credentials at rest
- Multi-bridge support (running multiple bridges simultaneously)
- `--push` deprecation path -- Alpha policy says breaking changes expected
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PROF-01 | Bridge config at team level -- bridge type and credentials validated against bridge's `schema.json` | `bridge::discover()` already reads `bridge` key from `botminter.yml`. Extend `ProfileManifest` with `bridges` list. Validation via `schema.json` using existing `serde_json` parsing. |
| PROF-02 | Profiles declare supported bridges in `bridges/` directory. Operator selects one (or none) during team setup. | `ProfileManifest` needs `bridges: Vec<BridgeDef>` field. Init wizard adds `cliclack::select` step. Telegram bridge already in `profiles/scrum-compact/bridges/telegram/`. |
| PROF-03 | `bm teams sync` provisions bridge resources (rooms, member identities) reusing bridge module. Generates `ralph.yml` RObot section based on active bridge config and member credentials. | `bridge::invoke_recipe()` handles provisioning. `workspace.rs` needs `ralph.yml` RObot section injection. Sync needs `--bridge` flag and per-member identity loop. |
| PROF-04 | Documentation updates for bridge abstraction, CLI commands, bridge spec, and profile bridge configuration | MkDocs site at `docs/content/`. New pages for bridge concepts, CLI reference updates, profile bridge config guide. |
| PROF-05 | `bm init` wizard offers bridge selection from profile's supported bridges, including "No bridge" | Init wizard uses `cliclack` (NOT `dialoguer`). Add selection step after profile selection. Add `--bridge` to non-interactive args. |
| PROF-06 | `scrum-compact-telegram` profile removed. Telegram added as supported bridge on `scrum-compact`. | Audit complete: `scrum-compact` already has `bridges/telegram/`. `scrum-compact-telegram` has no unique content. 26 files reference the old profile name. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| clap | 4 | CLI flag definitions (`--bridge`, `--repos`, `--all`) | Already in Cargo.toml |
| cliclack | 0.3 | Interactive wizard prompts (bridge selection in init) | Already used by init wizard |
| serde + serde_json | 1 | Bridge state, credential storage | Already in Cargo.toml |
| serde_yml | 0.0.12 | ProfileManifest extension, ralph.yml generation | Already in Cargo.toml |
| anyhow | 1 | Error handling | Already in Cargo.toml |
| comfy-table | 7 | Status display with bridge identity mapping | Already in Cargo.toml |
| chrono | 0.4 | Timestamps | Already in Cargo.toml |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| which | 7 | Check `just` availability before bridge provisioning | Already a dependency |
| tempfile | 3 | Temp dirs for bridge config exchange | Already a dependency |
| std::process::Command | stdlib | Invoke `just` recipes during sync | Bridge provisioning |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Removing `--push` immediately | Deprecation warning with redirect | Alpha policy explicitly says breaking changes expected. Remove immediately. |
| Per-member credential fields in config.yml | Formation-aware secret storage trait | Design trait now, implement local backend that stores in bridge-state.json. K8s backend comes later. |

**No new dependencies needed.** All required capabilities exist in the current dependency tree.

## Architecture Patterns

### Recommended Project Structure
```
crates/bm/src/
  bridge.rs              # Existing: add credential storage trait
  config.rs              # Add: bridge field to TeamEntry (or keep in botminter.yml only)
  profile.rs             # Add: bridges field to ProfileManifest, BridgeDef struct
  workspace.rs           # Add: ralph.yml RObot section generation
  cli.rs                 # Modify: TeamsSync flags, Init --bridge flag
  commands/
    init.rs              # Add: bridge selection wizard step, --bridge non-interactive flag
    hire.rs              # Add: optional bridge token prompt for external bridges
    teams.rs             # Redesign: sync() with --repos/--bridge/--all flags
    bridge.rs            # Existing: no changes needed
profiles/
  scrum-compact/
    botminter.yml        # Add: bridges declaration
    bridges/telegram/    # Already exists from Phase 8
  scrum/
    botminter.yml        # Add: bridges declaration
    bridges/telegram/    # Already exists from Phase 8
  scrum-compact-telegram/  # REMOVE entirely
docs/content/
  concepts/bridges.md    # NEW: bridge concepts
  reference/cli.md       # UPDATE: new sync flags, init --bridge
  how-to/bridge-setup.md # NEW: configuring bridges
```

### Pattern 1: ProfileManifest Bridge Declaration
**What:** Extend `botminter.yml` schema to declare supported bridges.
**When to use:** Profile authors adding bridge support.
**Example:**
```yaml
# botminter.yml (profile-level)
name: scrum-compact
# ... existing fields ...

bridges:
  - name: telegram
    display_name: "Telegram"
    description: "Telegram Bot API for team notifications"
    type: external
```

```rust
// profile.rs addition
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BridgeDef {
    pub name: String,
    pub display_name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub bridge_type: String,
}

// Add to ProfileManifest:
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub bridges: Vec<BridgeDef>,
```

### Pattern 2: Init Wizard Bridge Selection
**What:** After profile selection, present bridge choices from the profile's declared bridges.
**When to use:** Interactive `bm init` flow.
**Example:**
```rust
// commands/init.rs -- after profile selection
let manifest = profile::read_manifest(&selected_profile)?;

let bridge_selection: Option<String> = if !manifest.bridges.is_empty() {
    let mut items: Vec<(&str, &str, &str)> = manifest.bridges.iter()
        .map(|b| (b.name.as_str(), b.display_name.as_str(), b.description.as_str()))
        .collect();
    items.push(("none", "No bridge", "Run without a communication bridge"));

    let selected: &str = cliclack::select("Communication bridge?")
        .items(&items)
        .interact()?;

    if selected == "none" { None } else { Some(selected.to_string()) }
} else {
    None
};
```

### Pattern 3: Teams Sync with Bridge Provisioning
**What:** `bm teams sync --bridge` provisions per-member bridge identities and team room.
**When to use:** After workspace assembly, when `--bridge` or `--all` flag is set.
**Example:**
```rust
// commands/teams.rs -- sync() with new flags
pub fn sync(repos: bool, bridge_flag: bool, verbose: bool, team_flag: Option<&str>) -> Result<()> {
    // ... existing workspace assembly ...

    if bridge_flag {
        provision_bridge(&team_repo, &team.name, &cfg.workzone, &members)?;
    }
}

fn provision_bridge(
    team_repo: &Path,
    team_name: &str,
    workzone: &Path,
    members: &[String],
) -> Result<()> {
    let bridge_dir = match bridge::discover(team_repo, team_name)? {
        Some(dir) => dir,
        None => {
            println!("No bridge configured -- skipping bridge provisioning.");
            return Ok(());
        }
    };

    let manifest = bridge::load_manifest(&bridge_dir)?;
    let state_path = bridge::state_path(workzone, team_name);
    let mut state = bridge::load_state(&state_path)?;

    // Idempotent: only onboard members not yet provisioned
    for member in members {
        if state.identities.contains_key(member) {
            continue;
        }
        // For external bridges, check if credential is available
        if manifest.spec.bridge_type == "external" {
            let cred = bridge::resolve_credential(member, &state);
            if cred.is_none() {
                eprintln!("{}: no bridge credentials -- skipping. Use `bm bridge identity add` to add later.", member);
                continue;
            }
        }
        let result = bridge::invoke_recipe(&bridge_dir, &manifest.spec.identity.onboard, &[member], team_name)?;
        // ... store identity in state ...
    }

    // Create team room if missing
    if let Some(room_spec) = &manifest.spec.room {
        if state.rooms.is_empty() {
            let room_name = format!("{}-team", team_name);
            bridge::invoke_recipe(&bridge_dir, &room_spec.create, &[&room_name], team_name)?;
            // ... store room in state ...
        }
    }

    bridge::save_state(&state_path, &state)?;
    Ok(())
}
```

### Pattern 4: ralph.yml RObot Section Generation
**What:** During `bm teams sync`, generate the `RObot` section in each member's `ralph.yml` based on bridge config and per-member credentials.
**When to use:** workspace.rs `sync_workspace()` or `surface_member_files()`.
**Example:**
```rust
// workspace.rs -- inject RObot section into ralph.yml
fn inject_robot_section(
    ralph_yml_path: &Path,
    bridge_state: &BridgeState,
    member_name: &str,
) -> Result<()> {
    let contents = fs::read_to_string(ralph_yml_path)?;
    let mut doc: serde_yml::Value = serde_yml::from_str(&contents)?;

    if let Some(identity) = bridge_state.identities.get(member_name) {
        // Set RObot.enabled = true
        doc["RObot"]["enabled"] = serde_yml::Value::Bool(true);
        // Bridge-specific config will be injected via env vars at launch time
        // RObot section just enables the robot service
    } else {
        // No credentials -- disable RObot for this member
        doc["RObot"]["enabled"] = serde_yml::Value::Bool(false);
    }

    let output = serde_yml::to_string(&doc)?;
    fs::write(ralph_yml_path, output)?;
    Ok(())
}
```

### Pattern 5: Hire Command Token Prompt
**What:** During `bm hire`, if team has an external bridge configured, optionally prompt for the member's bridge token.
**When to use:** `bm hire <role>` when the team has an external bridge.
**Example:**
```rust
// commands/hire.rs -- after member directory creation
// Check if team has an external bridge configured
if let Some(bridge_dir) = bridge::discover(&team_repo, &team.name)? {
    let bridge_manifest = bridge::load_manifest(&bridge_dir)?;
    if bridge_manifest.spec.bridge_type == "external" {
        // Interactive: prompt for token
        if std::io::stdin().is_terminal() {
            let token: String = cliclack::input(
                &format!("{} bot token for {} (optional, enter to skip)",
                    bridge_manifest.metadata.display_name.as_deref().unwrap_or(&bridge_manifest.metadata.name),
                    member_name)
            ).default_input("").required(false).interact()?;

            if !token.is_empty() {
                // Store credential via formation-aware storage
                credential_store.store(&member_dir_name, &token)?;
            }
        }
    }
}
```

### Pattern 6: Formation-Aware Secret Storage (Trait Design)
**What:** Define a trait for credential storage that can be implemented by different formation backends.
**When to use:** Storing and retrieving per-member bridge tokens.
**Example:**
```rust
// bridge.rs or new credentials.rs module
pub trait CredentialStore {
    fn store(&self, member_name: &str, token: &str) -> Result<()>;
    fn retrieve(&self, member_name: &str) -> Result<Option<String>>;
    fn remove(&self, member_name: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<String>>;
}

/// Local formation backend -- stores in bridge-state.json
pub struct LocalCredentialStore {
    state_path: PathBuf,
}

impl CredentialStore for LocalCredentialStore {
    fn store(&self, member_name: &str, token: &str) -> Result<()> {
        let mut state = load_state(&self.state_path)?;
        // Upsert the identity with the token
        let identity = state.identities.entry(member_name.to_string())
            .or_insert_with(|| BridgeIdentity {
                username: member_name.to_string(),
                user_id: String::new(),
                token: String::new(),
                created_at: chrono::Utc::now().to_rfc3339(),
            });
        identity.token = token.to_string();
        save_state(&self.state_path, &state)
    }
    // ... other implementations
}
```

### Anti-Patterns to Avoid
- **Storing bridge selection in config.yml:** Bridge selection is team-level data, belongs in the team repo's `botminter.yml`. `config.yml` stores credentials and paths, not team configuration.
- **Prompting for bridge tokens during `bm init`:** Init is for team setup. Token collection happens during `bm hire` per-member. Init only records the bridge choice.
- **Coupling bridge provisioning to workspace assembly:** Keep `--bridge` as a separate flag from the default workspace sync. Provisioning can fail independently of workspace creation.
- **Modifying ralph.yml in the team repo:** RObot config is generated into workspace copies during sync, never written back to the team repo's member templates.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Interactive prompts | Custom stdin reading | `cliclack::select()` / `cliclack::input()` | Consistent with existing init wizard UX |
| CLI flag composition | Manual flag logic | clap `#[arg(conflicts_with)]` and groups | Handles mutual exclusion and help text |
| YAML manipulation | String concatenation | `serde_yml::Value` field mutation | Preserves formatting, handles edge cases |
| Profile bridge discovery | Custom directory walking | Add `bridges` field to `ProfileManifest` | Schema-declared is source of truth per CONTEXT.md |
| Credential priority | Custom env var logic | Existing `bridge::resolve_credential()` | Already handles env var -> state file priority |

**Key insight:** This phase is integration work -- connecting existing Phase 8 bridge module functions into existing CLI commands. No new external libraries or complex algorithms needed.

## Common Pitfalls

### Pitfall 1: ProfileManifest Deserialization Breaks Existing Profiles
**What goes wrong:** Adding `bridges: Vec<BridgeDef>` to `ProfileManifest` without `#[serde(default)]` causes deserialization failure for profiles that do not have a `bridges` field in their `botminter.yml`.
**Why it happens:** Serde requires all non-optional fields to be present unless defaulted.
**How to avoid:** Use `#[serde(default, skip_serializing_if = "Vec::is_empty")]` on the `bridges` field -- same pattern used for `projects`, `views`, and other optional fields in `ProfileManifest`.
**Warning signs:** `cargo test -p bm` fails immediately with deserialization errors on existing profile tests.

### Pitfall 2: Init Wizard Bridge Selection for Profiles Without Bridges
**What goes wrong:** The bridge selection step errors or shows an empty list when the profile has no bridges declared.
**Why it happens:** Not checking `manifest.bridges.is_empty()` before presenting the selection prompt.
**How to avoid:** Skip the bridge selection step entirely when the profile has no bridges. The "No bridge" state is the default.
**Warning signs:** Empty `cliclack::select` prompt that crashes or confuses users.

### Pitfall 3: Teams Sync --bridge Flag on External Bridge Without Credentials
**What goes wrong:** `bm teams sync --bridge` tries to onboard a member on a Telegram bridge but no bot token has been provided. The onboard recipe fails because `BM_BRIDGE_TOKEN_{USERNAME}` env var is not set and no token is in bridge state.
**Why it happens:** External bridges require operator-supplied tokens, unlike managed bridges that auto-create them.
**How to avoid:** Before invoking the onboard recipe for external bridges, check `bridge::resolve_credential()`. If None, print a warning and skip that member. The CONTEXT.md explicitly states: "Members without bridge credentials are flagged; operator can add later."
**Warning signs:** Bridge provisioning fails entirely because one member lacks credentials.

### Pitfall 4: ralph.yml RObot Section Overwrites Existing Config
**What goes wrong:** The sync process generates a `RObot` section in ralph.yml but clobbers existing non-bridge RObot settings (timeout, checkin interval) that were in the member template.
**Why it happens:** Naive file writing replaces the whole section instead of merging.
**How to avoid:** Load the existing ralph.yml as `serde_yml::Value`, merge the bridge-specific fields (enabled, backend-specific config) while preserving existing fields (timeout_seconds, checkin_interval_seconds). Or: only set `RObot.enabled` and use environment variables for all bridge-specific config at launch time.
**Warning signs:** Members lose custom RObot settings after sync.

### Pitfall 5: Removing scrum-compact-telegram Breaks References in 26 Files
**What goes wrong:** Deleting the profile directory without updating all references causes broken links in docs, failing tests, and stale code paths.
**Why it happens:** The profile name appears in 26 files across docs, tests, code, planning docs, and release notes.
**How to avoid:** Systematic grep for `scrum-compact-telegram` across the entire repo. Update or remove every reference. Run `cargo test -p bm` and `cargo clippy -p bm -- -D warnings` after cleanup.
**Warning signs:** Test failures in `cli_parsing`, `integration`, or `conformance` tests after deletion.

### Pitfall 6: --push Flag Removal Breaks Existing Scripts
**What goes wrong:** Removing `--push` from `bm teams sync` breaks any scripts or documentation that use the old flag.
**Why it happens:** Alpha policy says breaking changes are expected, but the Alpha users still have muscle memory and existing scripts.
**How to avoid:** Given Alpha policy, just remove it. But: update AGENTS.md (which documents `bm teams sync --push`), all docs, and the help text. The new `--repos` flag is the replacement.
**Warning signs:** AGENTS.md describes `--push` flag that no longer exists.

### Pitfall 7: Bridge Token Prompt During Hire in Non-Interactive Mode
**What goes wrong:** `bm hire` tries to prompt for a bridge token in non-interactive contexts (CI, scripts) and blocks or crashes.
**Why it happens:** Not checking `stdin().is_terminal()` before presenting interactive prompts.
**How to avoid:** Check `std::io::stdin().is_terminal()` (already used in the codebase) before presenting the token prompt. In non-interactive mode, skip the prompt silently -- the operator can use `bm bridge identity add` later.
**Warning signs:** CI pipelines hang waiting for input during `bm hire`.

## Code Examples

### ProfileManifest Extension
```rust
// Source: crates/bm/src/profile.rs -- extend existing struct
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProfileManifest {
    // ... existing fields ...

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<BridgeDef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BridgeDef {
    pub name: String,
    pub display_name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub bridge_type: String,
}
```

### Teams Sync Flag Redesign (cli.rs)
```rust
// Source: crates/bm/src/cli.rs -- replace existing TeamsCommand::Sync
Sync {
    /// Sync git repositories (replaces --push)
    #[arg(long)]
    repos: bool,

    /// Provision bridge identities and rooms
    #[arg(long)]
    bridge: bool,

    /// All remote operations (--repos --bridge)
    #[arg(short, long)]
    all: bool,

    /// Show detailed sync status per workspace
    #[arg(short, long)]
    verbose: bool,

    /// Team to operate on
    #[arg(short, long)]
    team: Option<String>,
},
```

### Init Non-Interactive Bridge Flag
```rust
// Source: crates/bm/src/cli.rs -- add to Init variant
Init {
    // ... existing fields ...

    /// Bridge to configure (optional, omit for no bridge)
    #[arg(long)]
    bridge: Option<String>,
}
```

### Team botminter.yml Bridge Recording
```rust
// After bridge selection in init wizard, write to team's botminter.yml
fn record_bridge_selection(team_repo: &Path, bridge_name: &str) -> Result<()> {
    let manifest_path = team_repo.join("botminter.yml");
    let contents = fs::read_to_string(&manifest_path)?;
    let mut doc: serde_yml::Value = serde_yml::from_str(&contents)?;

    doc["bridge"] = serde_yml::Value::String(bridge_name.to_string());

    let output = serde_yml::to_string(&doc)?;
    fs::write(&manifest_path, output)?;
    Ok(())
}
```

## scrum-compact-telegram Audit

### Differences from scrum-compact

| File | Difference | Unique Content? |
|------|-----------|-----------------|
| `botminter.yml` | Name/display_name/description only | NO -- same schema, labels, statuses, views |
| `PROCESS.md` | Older HIL docs (uses `human.interact` not GitHub comments) | NO -- `scrum-compact` has newer, better version |
| `knowledge/communication-protocols.md` | Older HIL docs (Telegram-based, not GitHub comments) | NO -- `scrum-compact` has evolved version |
| `roles/superman/ralph.yml` | `RObot.enabled: false` vs `RObot.enabled: true` with timeout/checkin | PARTIAL -- `scrum-compact` already has `RObot.enabled: true` with the same settings |
| `roles/superman/context.md` | Trivial differences | NO |
| `roles/team-manager/ralph.yml` | Differences in RObot section | NO -- same as above |
| `bridges/` directory | Only in `scrum-compact` | N/A -- `scrum-compact` has it, telegram profile does not |

**Conclusion:** `scrum-compact-telegram` has NO unique content that needs migration. It is a strictly older/inferior version of `scrum-compact`. The `scrum-compact` profile already has the Telegram bridge in `bridges/telegram/` and the more evolved GitHub-comment-based HIL pattern. Safe to delete entirely.

### Files Referencing scrum-compact-telegram (26 files)

Categories:
- **Planning docs** (ROADMAP, REQUIREMENTS, research, milestone history): Update to note removal
- **Docs** (MkDocs content: profiles, getting-started, reference): Remove references or update
- **Source code** (`profile.rs`): The profile name is embedded at compile time; deletion of the directory removes it from `include_dir`
- **README, RELEASE_NOTES**: Update references
- **AGENTS.md at project root**: No direct reference

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Separate profile per bridge (`scrum-compact-telegram`) | Bridge as profile option (in `bridges/` dir) | Phase 9 | One profile, N bridge choices |
| `--push` flag on teams sync | `--repos` / `--bridge` / `--all` flags | Phase 9 | Composable, self-documenting sync operations |
| Single team-wide Telegram token | Per-member bridge credentials | Phase 9 | Each member gets its own bot identity |
| RObot config hardcoded in ralph.yml template | RObot section generated during sync based on bridge | Phase 9 | Bridge-agnostic ralph.yml templates |
| Credentials in config.yml directly | Formation-aware credential storage abstraction | Phase 9 (design) | Future K8s backend plugs in without restructuring |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test framework (cargo test) |
| Config file | `crates/bm/Cargo.toml` |
| Quick run command | `cargo test -p bm` |
| Full suite command | `cargo test -p bm && cargo clippy -p bm -- -D warnings` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PROF-01 | ProfileManifest parses bridges field | unit | `cargo test -p bm profile::tests::manifest_with_bridges` | Wave 0 |
| PROF-01 | ProfileManifest parses without bridges (backward compat) | unit | `cargo test -p bm profile::tests::manifest_no_bridges` | Wave 0 |
| PROF-02 | Profile bridge discovery finds declared bridges | unit | `cargo test -p bm profile::tests::bridge_discovery` | Wave 0 |
| PROF-03 | Sync --bridge provisions member identities | integration | `cargo test -p bm --test integration sync_bridge_provision` | Wave 0 |
| PROF-03 | Sync --bridge skips members without credentials (external) | integration | `cargo test -p bm --test integration sync_bridge_skip_no_creds` | Wave 0 |
| PROF-03 | Sync --bridge generates RObot section in ralph.yml | integration | `cargo test -p bm --test integration sync_robot_section` | Wave 0 |
| PROF-05 | Init --non-interactive with --bridge flag records selection | integration | `cargo test -p bm --test integration init_with_bridge` | Wave 0 |
| PROF-05 | Init --non-interactive without --bridge defaults to no bridge | integration | `cargo test -p bm --test integration init_no_bridge` | Wave 0 |
| PROF-06 | scrum-compact-telegram profile removed (not in profile list) | unit | `cargo test -p bm profile::tests::no_telegram_profile` | Wave 0 |
| ALL | CLI parsing for new sync flags (--repos, --bridge, --all) | unit | `cargo test -p bm --test cli_parsing sync_flags` | Wave 0 |
| ALL | CLI parsing for init --bridge flag | unit | `cargo test -p bm --test cli_parsing init_bridge` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p bm`
- **Per wave merge:** `cargo test -p bm && cargo clippy -p bm -- -D warnings`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `ProfileManifest.bridges` field with unit tests for parsing with/without
- [ ] CLI parsing tests for new sync flags (`--repos`, `--bridge`, `--all`)
- [ ] CLI parsing test for init `--bridge` flag
- [ ] Integration tests for sync bridge provisioning (using stub bridge fixture)
- [ ] No framework install needed -- Rust test framework already configured

## Open Questions

1. **Bridge selection storage location**
   - What we know: CONTEXT.md says init records bridge selection in "team config." The `bridge::discover()` function reads `bridge` key from team repo's `botminter.yml`.
   - What's unclear: Whether to also store the bridge name in `~/.botminter/config.yml` (TeamEntry) for fast lookup, or always read from team repo's `botminter.yml`.
   - Recommendation: Store ONLY in team repo's `botminter.yml` (existing pattern). `bridge::discover()` already handles this. No change to `TeamEntry` in `config.yml`. This keeps the team repo as the single source of truth.

2. **Formation-aware credential store scope**
   - What we know: CONTEXT.md says "design the abstraction in Phase 9, implement local keyring backend."
   - What's unclear: Whether "local keyring" means a new Rust module or just the existing bridge-state.json storage pattern with a trait interface.
   - Recommendation: Design a `CredentialStore` trait in `bridge.rs` (or new `credentials.rs`). The Phase 9 "local" implementation stores tokens in bridge-state.json (which already happens). The trait is the forward-looking abstraction -- K8s formation provides a different implementation later. Keep it simple: `store()`, `retrieve()`, `remove()`, `list()`.

3. **ralph.yml RObot section: environment variables vs file injection**
   - What we know: `bm start` currently passes `RALPH_TELEGRAM_BOT_TOKEN` as an env var. Ralph resolves config from env -> file -> keychain.
   - What's unclear: Whether to inject bridge config into ralph.yml during sync or pass as env vars at launch.
   - Recommendation: Hybrid approach. Set `RObot.enabled: true` in ralph.yml during sync (so Ralph knows to start the robot service). Pass bridge-specific credentials as env vars at launch time (existing pattern, already works in `start.rs`). This avoids writing secrets to disk in ralph.yml.

4. **Per-member token env var naming for non-Telegram bridges**
   - What we know: Current pattern is `RALPH_TELEGRAM_BOT_TOKEN` (single team-wide token). External bridges need per-member tokens.
   - What's unclear: How to pass N different tokens when launching N Ralph instances.
   - Recommendation: Each `launch_ralph()` call already receives the specific member's workspace. Read that member's credential from bridge state at launch time and pass as `RALPH_TELEGRAM_BOT_TOKEN` (for Telegram) or the appropriate bridge-specific env var. One token per process, not N tokens globally.

## Sources

### Primary (HIGH confidence)
- `crates/bm/src/bridge.rs` -- Complete bridge module with manifest parsing, state management, recipe invocation, credential resolution
- `crates/bm/src/commands/bridge.rs` -- All `bm bridge` CLI handlers
- `crates/bm/src/cli.rs` -- Current CLI structure with `TeamsCommand::Sync { push }` and `BridgeCommand`
- `crates/bm/src/profile.rs` -- `ProfileManifest` struct (no bridges field currently), profile extraction, coding agent resolution
- `crates/bm/src/commands/init.rs` -- Interactive wizard using `cliclack`, non-interactive mode, team registration
- `crates/bm/src/commands/hire.rs` -- Member hiring flow, profile extraction, git commit
- `crates/bm/src/commands/teams.rs` -- `sync()` function with `--push` flag, workspace provisioning loop
- `crates/bm/src/commands/start.rs` -- `launch_ralph()` with env var injection, bridge auto-start
- `crates/bm/src/workspace.rs` -- Workspace creation and sync, file surfacing, ralph.yml handling
- `crates/bm/src/config.rs` -- `TeamEntry`, `Credentials` structs, config persistence
- `profiles/scrum-compact/botminter.yml` -- Current manifest without bridges field
- `profiles/scrum-compact/bridges/telegram/` -- Existing Telegram bridge implementation
- `profiles/scrum-compact-telegram/` -- Profile to be removed (full audit in this doc)

### Secondary (MEDIUM confidence)
- `.planning/research/ARCHITECTURE.md` -- System overview, data flow patterns
- `.planning/research/PITFALLS.md` -- Leaky abstraction, admin access, stdout corruption pitfalls
- `.planning/research/IMPLEMENTATION-NOTES.md` -- ralph.yml RObot convention, config resolution
- `.planning/phases/08-bridge-abstraction-cli/08-RESEARCH.md` -- Phase 8 patterns, bridge module design
- `.planning/phases/07-specs-foundation-bridge-contract/07-RESEARCH.md` -- Spec and ADR conventions

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in use, patterns well-established
- Architecture: HIGH -- every integration point directly observable in existing code
- Pitfalls: HIGH -- identified from codebase analysis and CONTEXT.md decisions
- scrum-compact-telegram audit: HIGH -- full diff-based comparison completed

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable codebase, no external dependency changes expected)
