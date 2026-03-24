# Profile Paths Must Match Workspace Model

Profile files under `profiles/` MUST use paths consistent with the workspace model that `bm teams sync` produces.

## Rule

All path references in profile files (ralph.yml hat instructions, context.md workspace diagrams, knowledge resolution tables, invariant scoping tables) MUST use the current workspace model paths. When the workspace model changes, all profile files MUST be updated in the same changeset.

The workspace model paths are defined by the `sync` command in `crates/bm/src/commands/sync.rs` and documented in the root `AGENTS.md` under "Workspace Model."

## Applies To

All files under `profiles/` that contain path references — primarily:
- `profiles/*/members/*/ralph.yml` — hat instruction paths for skills, knowledge, invariants
- `profiles/*/context.md` and `profiles/*/members/*/context.md` — workspace layout diagrams, resolution tables
- `profiles/*/AGENTS.md` and member-level equivalents

Does NOT apply to:
- Files under `specs/milestones/*/fixtures/` — historical snapshots
- Path-free files (e.g., `PROCESS.md` status definitions, knowledge prose)

## Examples

**Compliant:**

ralph.yml hat instruction referencing the current workspace model:
```yaml
instructions: |
  Read invariants from .botminter/invariants/ and .botminter/team/{{ member }}/invariants/
  Read knowledge from .botminter/knowledge/ and .botminter/projects/{{ project }}/knowledge/
```

context.md workspace layout matching sync output:
```
project-repo/
  .botminter/                    # Team repo clone
    knowledge/                   # Team knowledge
    invariants/                  # Team invariants
    team/<member>/               # Member config
  PROMPT.md → .botminter/...    # Symlinked
  AGENTS.md → .botminter/...    # Symlinked
  ralph.yml                      # Copied
```

**Violating:**

ralph.yml using a stale path prefix after workspace model changed from `.botminter/` to `team/` (or vice versa):
```yaml
instructions: |
  Read invariants from team/invariants/
  # ^^^ Wrong — workspace model currently uses .botminter/ prefix
```

context.md showing a workspace layout that doesn't match what `bm teams sync` produces:
```
project-repo/
  team/                          # ← Stale: should be .botminter/
    knowledge/
```

## Rationale

Profiles are embedded in the `bm` binary at compile time and are the templates delivered to users via `bm init`. When profile paths drift from the CLI's workspace model, agents in generated team repos look for files at paths that don't exist. This has happened multiple times during development (e.g., the `.botminter/` to `team/` migration in commits `b3e313c`, `d23f3f1`).
