# Phase 7: Specs Foundation & Bridge Contract - Research

**Researched:** 2026-03-08
**Domain:** Specification documents, ADR practice, conformance testing (no Rust runtime code)
**Confidence:** HIGH

## Summary

Phase 7 is a documentation-and-specification phase that produces zero Rust runtime code. All deliverables are markdown specification documents, MADR 4.0.0 ADRs, JSON schemas, YAML examples, and Rust conformance tests that validate spec artifacts structurally. The bridge plugin contract is formally specified so any developer can read the spec and build a conformant bridge implementation without reading BotMinter source code.

The phase has three workstreams: (1) establish ADR practice with MADR 4.0.0 in `.planning/adrs/`, including a meta-ADR and two substantive ADRs; (2) establish specs practice in `.planning/specs/` with a meta-spec and the bridge spec using RFC 2119 conformance language; (3) clean up the legacy `specs/` directory. The bridge spec is the centerpiece -- it defines `bridge.yml` format, `schema.json` contract, lifecycle operations, identity operations, and file-based config exchange. A stub/no-op bridge ships as the conformance test fixture.

**Primary recommendation:** Write the meta-ADR and meta-spec first (they define conventions), then the substantive ADRs and bridge spec in parallel, then conformance tests last (they validate the spec artifacts).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- MADR 4.0.0 format in `.planning/adrs/`
- Sequential integer numbering: `0001-title.md`, `0002-title.md`
- Include MADR template file for future ADRs
- First ADR (`0001`) is the meta-ADR: documents the ADR process itself -- format, conventions, numbering, lifecycle, when to write one, how to supersede. This is the single document anyone needs to read before writing an ADR.
- `README.md` in `.planning/adrs/` serves as the ADR index -- lists all ADRs with number, title, status, and date. Updated whenever an ADR is added or superseded.
- Two additional ADRs required: bridge abstraction design decisions, and Ralph robot backend decisions
- ADRs are immutable records -- superseded by new ADRs, not edited after acceptance
- `README.md` in `.planning/specs/` serves as the spec index -- lists all specs with name, status, and summary. Updated whenever a spec is added.
- Meta-spec document in `.planning/specs/` defines the spec discipline itself -- format conventions, when to write a spec, RFC 2119 usage, conformance levels, directory structure per spec, and how specs relate to ADRs.
- Spec document lives in `.planning/specs/bridge/`
- RFC 2119 conformance language (MUST, SHOULD, MAY)
- Single primary spec document covering: `bridge.yml` format, `schema.json` contract, lifecycle operations, identity operations, file-based config exchange
- Complete example `bridge.yml` and `schema.json` ship alongside the spec as reference
- Spec explicitly distinguishes local bridges (full lifecycle: start, stop, health) from external bridges (identity-only: onboard, rotate, remove)
- `bridge.yml` declares all integration points -- lifecycle commands, identity commands, config schema reference, bridge type
- Commands are Justfile recipes -- no hardcoded command names in the contract
- `schema.json` validates bridge-specific configuration values
- Config exchange uses file-based output: `$BRIDGE_CONFIG_DIR/config.json`, not stdout
- Identity commands: onboard, rotate-credentials, remove (per-user bot lifecycle)
- Lifecycle commands: start, stop, health (local bridges only)
- Rust conformance tests in `crates/bm/tests/` that parse `bridge.yml` and `schema.json` and validate structure against the spec
- A stub/no-op bridge implementation ships with the spec as the reference fixture for conformance testing
- Minimal scope: structure validation, command presence, schema validation -- not runtime behavior (that's Phase 8)
- Remove `specs/master-plan/`, `specs/milestones/`, `specs/prompts/`, `specs/tasks/` from tree
- `specs/design-principles.md` and `specs/presets/` also removed (design principles captured in PROJECT.md, presets are Ralph-specific)
- Preserved in git history -- no migration needed
- Every spec MUST include a non-goals section -- explicitly state what the spec does NOT cover
- Inline snippets in the spec text for illustration, plus complete working examples as separate files in the spec directory
- Conformance tests validate structure only -- `bridge.yml` and `schema.json` parse correctly and contain all required fields. No command execution at this phase.

### Claude's Discretion
None explicitly stated -- all decisions are locked.

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SPEC-01 | ADR practice established with MADR 4.0.0 format in `.planning/adrs/` with template and numbering convention | MADR 4.0.0 template verified (see Code Examples). Meta-ADR pattern well-established. |
| SPEC-02 | Bridge abstraction ADR documenting design decisions for the contract, lifecycle model, and config exchange | Extensive architecture research in ARCHITECTURE.md covers all design decisions. Bridge contract shape fully specified in CONTEXT.md decisions. |
| SPEC-03 | Bridge spec document in `.planning/specs/bridge/` with RFC 2119 conformance language | RFC 2119 keyword usage patterns documented. Knative-style spec structure researched. Bridge contract details locked in CONTEXT.md. |
| SPEC-04 | Minimal conformance test suite that validates a bridge implementation against the spec | Existing Rust test patterns in `crates/bm/tests/integration.rs` provide the model. Tests parse YAML/JSON and assert structure. |
| SPEC-05 | `.planning/adrs/` and `.planning/specs/` directories created. Existing top-level `specs/` directory contents removed | Current `specs/` directory contents identified: `design-principles.md`, `.gitignore`, `master-plan/`, `milestones/`, `presets/`, `prompts/`, `tasks/`. All to be removed. |
| BRDG-01 | Bridge definition file (`bridge.yml`) declares all integration points | Architecture research provides complete `bridge.yml` shape. CONTEXT.md locks: Justfile recipes, bridge type, config schema reference. |
| BRDG-02 | Bridge config schema (`schema.json`) validates bridge-specific configuration values | JSON Schema is the standard. Schema validates bridge-specific config before command invocation. |
| BRDG-03 | Bridge contract supports "external" bridges that skip start/stop and only implement identity management | CONTEXT.md locks the local vs external distinction. External bridges omit lifecycle commands. |
| BRDG-04 | Bridge identity management commands defined in `bridge.yml` (onboard, rotate-credentials, remove) | Identity commands are Justfile recipes per CONTEXT.md. Architecture research covers the per-agent identity pattern. |
| BRDG-07 | Config exchange between bridge commands and BotMinter uses file-based output (`$BRIDGE_CONFIG_DIR/config.json`), not stdout | File-based exchange explicitly chosen over stdout to avoid Pitfall 3 (stdout corruption). |
</phase_requirements>

## Standard Stack

### Core

This phase produces no runtime code. The "stack" is specification formats and test tooling.

| Tool/Format | Version | Purpose | Why Standard |
|-------------|---------|---------|--------------|
| MADR | 4.0.0 | ADR template format | Industry standard ADR format, maintained by the ADR community |
| RFC 2119 | N/A | Conformance keywords (MUST, SHOULD, MAY) | IETF standard for spec conformance language |
| JSON Schema | Draft 2020-12 | `schema.json` for bridge config validation | Standard schema validation format, Rust ecosystem support via `serde_json` |
| YAML (serde_yaml) | Already in Cargo.toml | Parse `bridge.yml` in conformance tests | Already used throughout BotMinter for profile manifests |
| Justfile | Already used | Bridge command recipes | Already used in BotMinter profiles (`formations/`) |

### Supporting

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `serde_json` | Parse and validate `schema.json` in conformance tests | Already a dependency |
| `serde_yaml` | Parse `bridge.yml` in conformance tests | Already a dependency |
| `tempfile` | Create isolated test directories for conformance tests | Already a dev-dependency |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| MADR 4.0.0 | Nygard-style ADR | MADR has richer structure (Decision Drivers, Pros/Cons per option). User locked MADR. |
| File-based config exchange | stdout JSON | stdout is fragile (Pitfall 3: diagnostic output corruption). File-based is more robust. User locked file-based. |
| Shell scripts for bridge commands | Rust trait/plugin system | Shell scripts match existing profile pattern. No recompilation for new backends. User locked Justfile recipes. |

**No new dependencies needed.** All parsing capabilities (`serde_yaml`, `serde_json`) are already in the BotMinter dependency tree.

## Architecture Patterns

### Recommended Directory Structure

```
.planning/
  adrs/
    README.md                    # ADR index (number, title, status, date)
    adr-template.md              # MADR 4.0.0 template for future ADRs
    0001-adr-process.md          # Meta-ADR: defines ADR practice itself
    0002-bridge-abstraction.md   # Bridge contract design decisions
    0003-ralph-robot-backend.md  # Ralph robot backend decisions
  specs/
    README.md                    # Spec index (name, status, summary)
    meta-spec.md                 # Defines spec discipline (format, RFC 2119 usage, etc.)
    bridge/
      bridge-spec.md             # Primary bridge spec document
      examples/
        bridge.yml               # Complete reference bridge.yml (local type)
        bridge-external.yml      # Complete reference bridge.yml (external type)
        schema.json              # Complete reference schema.json
        stub/                    # Stub/no-op bridge implementation
          bridge.yml             # Stub bridge manifest
          schema.json            # Stub bridge schema
          Justfile               # No-op recipes for all commands
crates/bm/tests/
  conformance.rs                 # Conformance tests validating spec artifacts
```

### Pattern 1: MADR 4.0.0 ADR Format

**What:** Architecture Decision Records using the MADR 4.0.0 template with YAML front matter.
**When to use:** Any architectural decision that affects the bridge contract, spec format, or project conventions.

**Template structure (verified from official MADR repository):**
```markdown
---
status: accepted
date: 2026-03-08
decision-makers: [list]
---

# {Short title of solved problem and solution}

## Context and Problem Statement

{2-3 sentence description}

## Decision Drivers

* {driver 1}
* {driver 2}

## Considered Options

* {option 1}
* {option 2}

## Decision Outcome

Chosen option: "{option}", because {justification}.

### Consequences

* Good, because {positive consequence}
* Bad, because {negative consequence}

### Confirmation

{How compliance will be validated}

## Pros and Cons of the Options

### {Option 1}

* Good, because {argument}
* Bad, because {argument}

### {Option 2}

* Good, because {argument}
* Bad, because {argument}

## More Information

{Links, timeline, related ADRs}
```

**Source:** [MADR 4.0.0 template](https://github.com/adr/madr/blob/main/template/adr-template.md) (HIGH confidence -- verified via WebFetch)

### Pattern 2: RFC 2119 Conformance Language in Specs

**What:** Using MUST, MUST NOT, SHOULD, SHOULD NOT, MAY as defined in RFC 2119 to express conformance requirements.
**When to use:** The bridge spec document and any future specs.

**Conventions (from RFC 2119 and Knative-style specs):**
- Keywords MUST be UPPERCASE when used as conformance terms
- The spec document MUST include the standard RFC 2119 boilerplate near the top:
  > The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.
- MUST = absolute requirement for conformance
- SHOULD = recommended but valid reasons to deviate may exist
- MAY = truly optional, implementors decide

**Source:** [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) (HIGH confidence)

### Pattern 3: bridge.yml Manifest Structure

**What:** YAML manifest declaring bridge integration points, using Justfile recipes for commands.
**When to use:** Defining the bridge plugin contract.

**Example (local bridge type):**
```yaml
# bridge.yml - Bridge plugin manifest
apiVersion: botminter.dev/v1alpha1
kind: Bridge
metadata:
  name: rocketchat
  displayName: "Rocket.Chat"
  description: "Self-hosted team chat via Rocket.Chat"

spec:
  type: local  # "local" (full lifecycle) or "external" (identity-only)

  # Config schema reference -- validated before any command invocation
  configSchema: schema.json

  # Lifecycle commands (local bridges only)
  lifecycle:
    start: start          # Justfile recipe name
    stop: stop            # Justfile recipe name
    health: health        # Justfile recipe name

  # Identity commands (all bridges)
  identity:
    onboard: onboard              # Justfile recipe name
    rotate-credentials: rotate    # Justfile recipe name
    remove: remove                # Justfile recipe name

  # Config exchange
  configDir: "$BRIDGE_CONFIG_DIR"  # Commands write config.json here
```

**Example (external bridge type):**
```yaml
apiVersion: botminter.dev/v1alpha1
kind: Bridge
metadata:
  name: telegram
  displayName: "Telegram"
  description: "Telegram bot integration (external service)"

spec:
  type: external  # No lifecycle commands
  configSchema: schema.json

  identity:
    onboard: onboard
    rotate-credentials: rotate
    remove: remove

  configDir: "$BRIDGE_CONFIG_DIR"
```

### Pattern 4: Conformance Test Structure

**What:** Rust tests in `crates/bm/tests/conformance.rs` that parse spec artifacts and validate structure.
**When to use:** Validating that example bridge implementations conform to the spec.

**Follows existing test patterns from `integration.rs`:**
```rust
// Conformance tests parse bridge.yml and schema.json from the stub bridge
// and validate structural correctness against the spec.

use serde_yaml;
use serde_json;
use std::path::Path;

#[test]
fn stub_bridge_yml_parses_and_has_required_fields() {
    let content = std::fs::read_to_string(
        "path/to/stub/bridge.yml"
    ).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();

    // Validate required top-level fields
    assert!(doc["apiVersion"].is_string());
    assert!(doc["kind"].as_str() == Some("Bridge"));
    assert!(doc["metadata"]["name"].is_string());
    assert!(doc["spec"]["type"].is_string());
    assert!(doc["spec"]["configSchema"].is_string());
    assert!(doc["spec"]["identity"]["onboard"].is_string());
    // ... etc
}

#[test]
fn stub_schema_json_is_valid_json_schema() {
    let content = std::fs::read_to_string(
        "path/to/stub/schema.json"
    ).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Validate it looks like a JSON Schema
    assert!(schema["$schema"].is_string());
    assert!(schema["type"].as_str() == Some("object"));
    assert!(schema["properties"].is_object());
}
```

### Anti-Patterns to Avoid

- **Over-specifying in ADRs:** ADRs document decisions and rationale, not implementation details. The bridge spec covers the "what"; ADRs cover the "why".
- **Mixing spec levels:** The bridge spec defines the contract. Conformance tests validate structure only. Runtime behavior testing is Phase 8. Do not blur these boundaries.
- **Specifying implementation details:** The spec defines the contract shape (what fields exist, what types they are, what commands are required). It does NOT define how commands are implemented internally.
- **Editing accepted ADRs:** ADRs are immutable once accepted. Write a new ADR that supersedes the old one.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| YAML parsing in tests | Custom YAML parser | `serde_yaml::Value` | Already a dependency, handles edge cases |
| JSON Schema structure | Custom validator | `serde_json::Value` field assertions | Full JSON Schema validation is Phase 8 scope; structure validation is sufficient now |
| ADR template | Custom format | MADR 4.0.0 template verbatim | Industry standard, well-documented, user-locked choice |
| RFC 2119 boilerplate | Custom conformance language | Standard RFC 2119 paragraph | Universally understood, no ambiguity |

**Key insight:** This phase is specification documents. The main risk is over-engineering -- adding code that belongs in later phases. The conformance tests should be minimal structural assertions, not a full validation framework.

## Common Pitfalls

### Pitfall 1: Spec Scope Creep Into Runtime Behavior
**What goes wrong:** The bridge spec starts defining how commands are invoked, error handling semantics, retry policies, or health check intervals -- things that belong in Phase 8 (bridge CLI implementation).
**Why it happens:** Natural desire to be "complete." The spec feels unfinished without runtime behavior.
**How to avoid:** The non-goals section explicitly states: "This spec does NOT define command invocation mechanisms, error handling, retry policies, or runtime behavior. Those are implementation concerns addressed in Phase 8."
**Warning signs:** If the spec mentions `std::process::Command`, `tokio`, error codes, or retry counts, it is leaking into runtime territory.

### Pitfall 2: Conformance Tests That Are Actually Integration Tests
**What goes wrong:** Conformance tests start invoking Justfile recipes, spawning processes, or testing runtime behavior instead of just parsing YAML/JSON and checking field presence.
**Why it happens:** "Conformance testing" sounds like it should test actual behavior.
**How to avoid:** Conformance tests at this phase ONLY: (1) parse `bridge.yml` as YAML, (2) parse `schema.json` as JSON, (3) assert required fields exist with correct types, (4) validate that lifecycle commands are present for local bridges and absent (or optional) for external bridges.
**Warning signs:** If a test calls `Command::new("just")`, it has crossed the line.

### Pitfall 3: ADR Paralysis
**What goes wrong:** Spending excessive time perfecting ADR wording instead of moving to the bridge spec. The meta-ADR becomes a research project about ADR best practices.
**Why it happens:** ADRs are a new practice for this project. First-time ADR authors over-invest in getting the format "right."
**How to avoid:** Time-box ADR writing. The meta-ADR should be 1-2 pages. The substantive ADRs should each be 1-2 pages. The MADR template provides the structure -- fill it in, do not reinvent it.
**Warning signs:** If the meta-ADR is longer than the bridge spec, priorities are inverted.

### Pitfall 4: Legacy specs/ Removal Breaks References
**What goes wrong:** Removing `specs/` directory contents breaks references in AGENTS.md, README.md, or other project documentation.
**Why it happens:** `specs/master-plan/` is referenced in AGENTS.md Key Directories table and elsewhere.
**How to avoid:** After removing `specs/` contents, grep the entire repo for `specs/master-plan`, `specs/milestones`, `specs/prompts`, `specs/tasks`, `specs/design-principles`, `specs/presets` and update or remove all references. Update AGENTS.md Key Directories table.
**Warning signs:** Broken links or stale references in project documentation.

### Pitfall 5: bridge.yml Schema Too Rigid or Too Loose
**What goes wrong:** The `bridge.yml` schema is either so rigid that adding a new bridge requires spec changes, or so loose that implementations can omit critical fields without being caught.
**Why it happens:** First spec iteration. No implementation experience to calibrate against.
**How to avoid:** Follow the local/external distinction strictly. Local bridges MUST have lifecycle commands. All bridges MUST have identity commands. Use `apiVersion` for future evolution. The stub bridge is the minimum conformant implementation.
**Warning signs:** If the stub bridge needs special cases to pass conformance, the spec is too rigid.

## Code Examples

### MADR 4.0.0 Meta-ADR Example

```markdown
---
status: accepted
date: 2026-03-08
---

# Use MADR 4.0.0 for Architecture Decision Records

## Context and Problem Statement

BotMinter needs a systematic way to document architectural decisions so future
contributors understand the rationale behind design choices. How should we
record these decisions?

## Decision Drivers

* Decisions must be discoverable alongside the code
* Format must be lightweight (markdown, no tooling required)
* Must support decision lifecycle (proposed, accepted, superseded)

## Considered Options

* MADR 4.0.0 (Markdown Any Decision Record)
* Nygard-style ADR (original 2011 format)
* Y-statements
* No formal ADR practice

## Decision Outcome

Chosen option: "MADR 4.0.0", because it provides structured sections
(Decision Drivers, Considered Options with Pros/Cons) that make decisions
self-documenting without requiring external context.

### Consequences

* Good, because MADR 4.0.0 is the most widely adopted ADR template
* Good, because the optional sections allow lightweight ADRs for simple decisions
* Neutral, because the template is slightly more verbose than Nygard-style
* Bad, because team members need to learn the format (mitigated by the template file)

### Confirmation

The existence of this ADR (0001) and subsequent ADRs using the MADR format
confirms adoption. Conformance tests validate that ADR files follow the template.
```

### RFC 2119 Usage in Bridge Spec

```markdown
## Conformance

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
"SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be
interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

### Bridge Manifest (bridge.yml)

A conformant bridge MUST include a `bridge.yml` file at the bridge root directory.

The manifest MUST contain the following top-level fields:
- `apiVersion`: MUST be `botminter.dev/v1alpha1`
- `kind`: MUST be `Bridge`
- `metadata.name`: MUST be a non-empty string matching `[a-z][a-z0-9-]*`
- `spec.type`: MUST be either `local` or `external`
- `spec.configSchema`: MUST reference a valid JSON Schema file

A bridge of type `local` MUST declare lifecycle commands:
- `spec.lifecycle.start`: Justfile recipe name
- `spec.lifecycle.stop`: Justfile recipe name
- `spec.lifecycle.health`: Justfile recipe name

A bridge of type `external` MUST NOT declare lifecycle commands.

All bridges MUST declare identity commands:
- `spec.identity.onboard`: Justfile recipe name
- `spec.identity.rotate-credentials`: Justfile recipe name
- `spec.identity.remove`: Justfile recipe name
```

### Stub Bridge Justfile

```makefile
# Stub/no-op bridge implementation -- conformance test fixture
# All recipes exit 0 and produce minimal valid output

start:
    @mkdir -p "$BRIDGE_CONFIG_DIR"
    @echo '{"url": "http://localhost:0", "status": "stub"}' > "$BRIDGE_CONFIG_DIR/config.json"

stop:
    @echo "stub: stop (no-op)" >&2

health:
    @echo "stub: healthy" >&2

onboard username:
    @mkdir -p "$BRIDGE_CONFIG_DIR"
    @echo '{"username": "{{username}}", "user_id": "stub-id", "token": "stub-token"}' > "$BRIDGE_CONFIG_DIR/config.json"

rotate username:
    @mkdir -p "$BRIDGE_CONFIG_DIR"
    @echo '{"username": "{{username}}", "user_id": "stub-id", "token": "rotated-stub-token"}' > "$BRIDGE_CONFIG_DIR/config.json"

remove username:
    @echo "stub: removed {{username}}" >&2
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| stdout JSON config exchange | File-based config exchange (`$BRIDGE_CONFIG_DIR/config.json`) | Decision in this phase | Avoids stdout corruption pitfall (Pitfall 3 from milestone research) |
| Shell scripts as bridge commands | Justfile recipes | Decision in this phase | Leverages existing Justfile pattern from profiles |
| No ADR practice | MADR 4.0.0 in `.planning/adrs/` | This phase | Formalizes architectural decision tracking |
| Specs in `specs/` directory | Specs in `.planning/specs/` | This phase | Consolidates planning artifacts under `.planning/` |
| Legacy PDD artifacts in `specs/` | Removed (git history only) | This phase | Cleans up stale directory structure |

**Deprecated/outdated:**
- `specs/master-plan/`: Legacy PDD artifacts, replaced by `.planning/` structure
- `specs/milestones/`: Legacy milestone tracking, superseded by `.planning/milestones/`
- `specs/prompts/`: Reusable planning prompts, may be recreated under `.planning/` if needed
- `specs/design-principles.md`: Content captured in PROJECT.md
- `specs/presets/`: Ralph-specific, not BotMinter scope

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test framework (cargo test) |
| Config file | `crates/bm/Cargo.toml` (existing) |
| Quick run command | `cargo test -p bm conformance` |
| Full suite command | `cargo test -p bm` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SPEC-04 | Stub bridge.yml parses with required fields | unit | `cargo test -p bm conformance::bridge_yml -x` | Wave 0 |
| SPEC-04 | Stub schema.json is valid JSON with required structure | unit | `cargo test -p bm conformance::schema_json -x` | Wave 0 |
| BRDG-01 | bridge.yml declares lifecycle + identity commands | unit | `cargo test -p bm conformance::bridge_commands -x` | Wave 0 |
| BRDG-02 | schema.json has valid JSON Schema structure | unit | `cargo test -p bm conformance::schema_structure -x` | Wave 0 |
| BRDG-03 | External bridge omits lifecycle, has identity commands | unit | `cargo test -p bm conformance::external_bridge -x` | Wave 0 |
| BRDG-04 | Identity commands (onboard, rotate, remove) present in bridge.yml | unit | `cargo test -p bm conformance::identity_commands -x` | Wave 0 |
| BRDG-07 | Spec document references file-based config exchange | manual-only | Review bridge-spec.md text | N/A |
| SPEC-01 | ADR files exist with MADR 4.0.0 structure | manual-only | Verify file existence and format | N/A |
| SPEC-02 | Bridge abstraction ADR exists with required sections | manual-only | Verify file existence and content | N/A |
| SPEC-03 | Bridge spec uses RFC 2119 language | manual-only | Verify spec document text | N/A |
| SPEC-05 | Legacy specs/ contents removed, .planning/adrs/ and .planning/specs/ created | manual-only | Verify directory structure | N/A |

### Sampling Rate
- **Per task commit:** `cargo test -p bm conformance`
- **Per wave merge:** `cargo test -p bm`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/bm/tests/conformance.rs` -- new test file for spec conformance tests
- [ ] `.planning/specs/bridge/examples/stub/` -- stub bridge fixture for tests
- [ ] No framework install needed -- Rust test framework already configured

## Open Questions

1. **bridge.yml apiVersion format**
   - What we know: Knative uses `serving.knative.dev/v1`. The CONTEXT.md does not specify an apiVersion convention.
   - What's unclear: Whether to use `botminter.dev/v1alpha1` or a simpler `v1alpha1` string.
   - Recommendation: Use `botminter.dev/v1alpha1` to follow Kubernetes/Knative convention and leave room for version evolution. Decide in the bridge abstraction ADR.

2. **Justfile recipe argument passing**
   - What we know: Identity commands need arguments (e.g., `onboard` takes a username). Justfile supports recipe arguments.
   - What's unclear: Exact environment variables available to recipes vs. recipe arguments.
   - Recommendation: Spec should define both: recipe arguments for command-specific input (username), environment variables for context (`$BRIDGE_CONFIG_DIR`, `$BM_TEAM_NAME`). Document in the spec.

3. **schema.json draft version**
   - What we know: JSON Schema has multiple drafts (Draft 4, 7, 2019-09, 2020-12).
   - What's unclear: Which draft the spec should mandate.
   - Recommendation: Use JSON Schema Draft 2020-12 (current). Keep schemas simple (type, properties, required) to avoid draft-specific features. The conformance test only checks structure, not validates against the schema.

## Sources

### Primary (HIGH confidence)
- [MADR 4.0.0 template](https://github.com/adr/madr/blob/main/template/adr-template.md) -- verified via WebFetch, complete template structure confirmed
- [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) -- IETF standard for conformance keywords
- BotMinter source: `crates/bm/tests/integration.rs` -- existing test patterns (tempdir, serde_yaml parsing, Command-based assertions)
- BotMinter source: `crates/bm/src/profile.rs` -- existing YAML manifest parsing patterns (schema_version, ProfileManifest)
- `.planning/research/ARCHITECTURE.md` -- bridge contract design, component boundaries, data flow
- `.planning/research/RALPH-ROBOT-INTERNALS.md` -- RobotService trait, RobotConfig structure, wiring patterns
- `.planning/research/PITFALLS.md` -- stdout corruption (Pitfall 3), leaky abstraction (Pitfall 1), ADR integration (Pitfall 11)

### Secondary (MEDIUM confidence)
- [Knative API Specification](https://github.com/knative/specs/blob/main/specs/serving/knative-api-specification-1.0.md) -- spec format conventions, RFC 2119 usage patterns (rate-limited during fetch but structure known from training data)
- `.planning/research/FEATURES.md` -- feature landscape, MVP recommendation, dependency graph

### Tertiary (LOW confidence)
- JSON Schema Draft 2020-12 specifics -- recommend but did not verify specific draft features needed for bridge schemas

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all tools already in dependency tree, MADR template verified
- Architecture: HIGH -- directory structure and file formats locked in CONTEXT.md, no ambiguity
- Pitfalls: HIGH -- well-documented from milestone research, directly applicable
- Conformance tests: HIGH -- follows existing integration.rs patterns with serde_yaml/serde_json

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable -- specification formats do not change frequently)
