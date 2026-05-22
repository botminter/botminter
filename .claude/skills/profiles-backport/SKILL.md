---
name: profiles-backport
description: >-
  Backport improvements from a running team repo to its source profile.
  Use when asked to "backport to profile", "upstream team changes",
  "harvest improvements", "reverse-sync to profile", or "/profiles-backport".
  Do NOT use for profile-to-team sync (workspace-sync), member config
  tuning (member-tuning), or process changes (process-evolution).
metadata:
  author: botminter
  version: 1.0.0
  category: operations
---

# Profiles Backport

Propagate generic improvements from a running team repo back to the source
profile so that new teams created via `bm init` inherit the improvements.
This is the reverse direction of the workspace-sync skill's `apply-profile`
operation.

## When to Use

- After a stabilization period, the operator wants to harvest improvements
- A retrospective action item requests a profile backport
- Significant operational improvements (bug fixes, robustness changes, new
  capabilities, process refinements) have accumulated in a team repo
- The operator says "backport to profile", "push changes upstream", "harvest
  improvements from the team repo"

## Parameters

- **team_repo_path** (required): Absolute path to the team repo instance to
  harvest from. Example: `~/workspaces/may-team/team/`
- **profile_name** (optional): Profile to backport to. If omitted, auto-detect
  from `<team_repo_path>/botminter.yml` field `profile:`.
- **scope** (optional): Restrict to specific artifact types. One or more of:
  `scripts`, `skills`, `hats`, `context`, `process`, `knowledge`,
  `ralph-prompts`, `workflows`, `botminter-yml`. If omitted, scan all.

## Quick Start

1. **Orient** — Confirm CWD is the botminter project root (check for
   `profiles/` and `Cargo.toml`). Resolve the team repo path and target
   profile. Map each hired member to its role.
2. **Diff** — Compare the team repo against the profile source across all
   artifact types. Classify each change as generic (backport), team-specific
   (skip), or ambiguous (ask).
3. **Backport** — Apply generic changes using the appropriate strategy per
   artifact type. Reverse-template concrete values back to profile
   placeholders.
4. **Verify** — Run the verification checklist. Run `just clippy && just test`.
   Both MUST pass before committing.

## Reverse-Templating Reference

The workspace-sync skill (forward direction) defines 7 transformation rules.
Backporting applies the exact inverse. The workspace-sync SKILL.md at
`profiles/<name>/roles/chief-of-staff/coding-agent/skills/workspace-sync/SKILL.md`
is the authoritative source for forward rules.

| # | Forward Rule (workspace-sync) | Reverse Operation (backporting) |
|---|-------------------------------|--------------------------------|
| 1 | **Agent tag filtering**: include `+agent:claude-code` content, strip tag lines | **Restore tag pairs**: wrap claude-code-specific content in `<!-- +agent:claude-code -->` / `<!-- -agent -->`. If content applies to all agents, do NOT add tags |
| 2 | **context.md → CLAUDE.md rename** | **CLAUDE.md → context.md rename**: any file named `CLAUDE.md` becomes `context.md` in the profile |
| 3 | **Member placeholders rendered**: `{{member_dir}}` → `engineer-bob`, `{{role}}` → `engineer`, `{{member_name}}` → `bob` | **Concrete values → placeholders**: replace all member directory names with `{{member_dir}}`, role names with `{{role}}`, display names with `{{member_name}}`. Only in path patterns — do NOT global-replace in prose content |
| 4 | **`<project>` expanded**: single `<project>` entry becomes one entry per registered project | **Collapse project entries**: multiple concrete project entries (`team/projects/botminter/...`, `team/projects/hypershift/...`) collapse back to single `team/projects/<project>/...` template. In documentation contexts, `<project>` stays as-is |
| 5 | **Directory exclusion**: `roles/` and `.schema/` skipped during forward sync | **Directory mapping**: `members/<member>/` content maps back to `roles/<role>/`. The `.schema/` directory only exists in the profile source |
| 6 | **Manifest finalization**: `.botminter.yml` → `botminter.yml` with `name` field added | **Manifest reversal**: `botminter.yml` → `.botminter.yml` with `name` field stripped |
| 7 | **Brain prompt rendering**: variables rendered into `brain-prompt.md` | **No reverse needed**: brain prompt is runtime-only, not backported |

**Rule 3 heuristic (path vs prose):** Path patterns are strings containing
`/` characters or appearing in YAML keys like `dirs:`, `path:`, `file:`.
Prose content is running natural-language text in `instructions:` values.
When ambiguous, replace and verify the result renders correctly with
different member names.

### Detecting Values for Reverse-Templating

Read `<team_repo_path>/members/<member>/botminter.yml` for each hired member
to get the concrete values:

```yaml
name: bob           # → {{member_name}}
role: engineer      # → {{role}}
# directory name    # → {{member_dir}} (e.g., engineer-bob)
```

Read `<team_repo_path>/botminter.yml` for the project list under `projects:`.

### BM Injection Markers

The profile uses paired comment markers for content injected by `bm` at
hire/sync time:

```markdown
<!-- BM:PROJECT_CONTEXT -->
...injected content...
<!-- /BM:PROJECT_CONTEXT -->
```

Markers: `PROJECT_CONTEXT`, `WORKSPACE_LAYOUT`, `PROJECT_KNOWLEDGE`,
`PROJECT_INVARIANTS`, `PROJECT_SKILL_DIRS`.

In YAML files (ralph.yml), markers use comment syntax:
`# <!-- BM:PROJECT_SKILL_DIRS -->` / `# <!-- /BM:PROJECT_SKILL_DIRS -->`.
The marker names are the same as in markdown.

**Reverse**: Remove all content between marker pairs. Restore empty marker
pairs as they appear in the profile source. Content OUTSIDE markers that was
added in the team repo is a candidate for backporting.

## Artifact Reference

### Shell Scripts

**Location**: Both team-level (`coding-agent/skills/*/scripts/`) and
role-level (`roles/*/coding-agent/skills/*/scripts/`).
**Strategy**: Diff and apply.

1. Diff each script in the team repo against the profile source
2. Verify each change is generic (no team-specific values)
3. Apply generic changes to the profile source
4. Common improvements: `jq --arg` fixes, error handling, retry logic,
   helper functions, validation checks

### SKILL.md Files

**Location**: `coding-agent/skills/*/SKILL.md` and
`roles/*/coding-agent/skills/*/SKILL.md`
**Strategy**: Section-by-section diff and apply.

1. Diff the team version against the profile version
2. Review changes per section (frontmatter, overview, steps, troubleshooting)
3. Verify content is generic
4. Apply generic changes
5. Increment `metadata.version` patch number if changes are applied
6. For skills shared across roles (member-tuning, process-evolution, etc.),
   apply to ALL role copies

### ralph.yml (Copy-Reverse-Template-Diff)

**Location**: `roles/*/ralph.yml`
**Strategy**: Copy → reverse-template → diff → verify → apply.

This is the most complex artifact. Hat instructions embed member-specific
paths throughout.

1. **Copy**: Take the team member's ralph.yml verbatim as a working copy
2. **Reverse-template**: Apply Rules 1-6:
   - Replace concrete member names → `{{member_dir}}`
   - Collapse project-specific entries → `<project>` template form
   - Restore `# <!-- BM:PROJECT_SKILL_DIRS -->` markers in `skills.dirs`
   - Preserve `# +agent:claude-code` blocks
3. **Diff**: Compare the reverse-templated copy against the profile ralph.yml
4. **Review**: Focus on semantic sections — hat instructions, triggers,
   publishes, guardrails. Ignore YAML formatting noise (quoting style,
   indentation differences from serialization tools)
5. **Verify**: Every hunk must be intentional. Grep for leaked concrete values
6. **Apply**: Replace the profile ralph.yml with the verified result
7. **Validate**: Parse with a YAML validator

**Per-role**: Repeat for each role (engineer, chief-of-staff, sentinel). Use
the member-to-role mapping from orientation.

### context.md / CLAUDE.md

**Location**: `roles/*/context.md` (profile) / `members/*/CLAUDE.md` (team)
**Strategy**: Enrich + reverse-template + agent tags.

1. Apply Rule 2 rename: `CLAUDE.md` → `context.md`
2. Strip BM injection marker content — restore empty marker pairs
3. Reverse-template member and project references (Rules 3, 4)
4. Restore `<!-- +agent:claude-code -->` tags around claude-code-specific
   content (Rule 1 reverse)
5. Diff against the profile's context.md
6. Apply generic improvements

Also applies to the team-level `CLAUDE.md` → profile `context.md`.

### PROCESS.md

**Location**: profile root
**Strategy**: Diff and apply new sections.

1. Diff team `PROCESS.md` against profile `PROCESS.md`
2. Apply generic additions and improvements
3. Skip team-specific process modifications

### Knowledge Documents

**Location**: `knowledge/`
**Strategy**: Selective backport.

1. Diff `<team_repo>/knowledge/` against `profiles/<name>/knowledge/`
2. New files: classify as generic (backport) or team-specific (skip)
3. Modified files: diff and classify hunks
4. Deleted files: flag for review (may be intentional team customization)

### Workflows (.dot files)

**Location**: `workflows/`
**Strategy**: Direct diff and apply. Workflow files are typically generic.

### botminter.yml

**Location**: profile root
**Strategy**: Selective field merge.

1. Diff team `botminter.yml` against profile `botminter.yml`
2. Skip team-specific fields: `name`, `projects`, member entries, bridge
   config, operator config
3. Backport generic fields: new labels, new statuses, updated descriptions,
   modified meetings, wording fixes

### ralph-prompts/

**Location**: `ralph-prompts/`
**Strategy**: Diff and apply. Files here (guardrails, hat templates,
orientation, reference docs) are generic.

### brain/system-prompt.md

**Location**: `brain/system-prompt.md`
**Strategy**: Diff and apply, ignoring rendered variable changes.

1. Diff the team repo's `brain/system-prompt.md` against the profile's
2. Skip changes that are just rendered variables (`{{team_name}}`,
   `{{member_name}}`, etc.) — those are runtime values, not content
3. Backport structural or prose improvements to the template itself

## Cross-Member Propagation

**This check is MANDATORY.** After identifying the delta for each member,
compare improvements across members to find changes made to one that should
apply to others.

Check:
- **Same-role members**: If two engineers exist, compare their ralph.yml,
  context.md, and skills for drift
- **Shared skills across roles**: member-tuning, process-evolution,
  retrospective, role-management, team-design exist in both engineer and
  chief-of-staff. Changes to one role's copy must be checked against the other
- **Board-scanner**: exists at team-level AND role-level overrides for
  chief-of-staff and sentinel. Improvements to one copy (e.g., reconciliation
  step) must propagate to all copies. Dispatch tables are role-specific — do
  NOT propagate dispatch reordering across roles
- **Role-specific context.md**: A bug fix in one role's workspace orientation
  (e.g., chief-of-staff still having old "your working directory is the team
  repository" text) needs fixing across all roles

Produce a propagation report listing: what changed, which members have it,
which need it.

## Cross-Profile Propagation

After backporting to one profile, check if shared artifacts should propagate
to sibling profiles. Run `ls profiles/` to list all profiles.

**Shared artifacts** (likely need cross-profile propagation):
- `coding-agent/skills/github-project/` (scripts and SKILL.md)
- `coding-agent/skills/status-workflow/`
- `coding-agent/skills/board-scanner/` (team-level copy — not all profiles have this; check before propagating)
- `knowledge/` documents
- `invariants/`
- `bridges/`
- `formations/`
- `ralph-prompts/`
- `workflows/`
- `specs/` (spec index and structure)
- `skills/` (top-level profile-scoped skills, e.g., knowledge-manager)
- `brain/` (system-prompt.md template — see artifact reference below)

**Role-specific artifacts** (do NOT propagate — roles differ across profiles):
- `roles/*/ralph.yml`
- `roles/*/context.md`
- `roles/*/coding-agent/skills/` (role-level skills)
- `roles/*/hats/`

For each shared artifact modified, diff it against sibling profiles and offer
to apply the same change.

## Classification Guide

When classifying a change as generic or team-specific:

| Signal | Classification |
|--------|---------------|
| Bug fix in a script (error handling, edge case, injection prevention) | Generic — backport |
| Improved hat instructions (clearer wording, better flow, condensation) | Generic — backport |
| New skill section or capability | Generic — backport |
| Robustness improvement (retry logic, validation, recovery) | Generic — backport |
| Process documentation addition (lifecycle tables, conventions) | Generic — backport |
| New team-level knowledge document with universal topic | Generic — backport |
| Member names, project names, org names in content | Team-specific — reverse-template or skip |
| Project-specific merge-gate knowledge | Team-specific — skip |
| Project-specific test infrastructure or env vars | Team-specific — skip |
| Reordering of existing content (e.g., priority table) | Ambiguous — present to human with the WHY |
| New section referencing team-specific context | Ambiguous — may need genericization |

For ambiguous changes, explain what problem the change solved in the team
repo so the human can make an informed decision.

## Verification Checklist

ALL of these MUST pass before committing.

### 1. Leaked Value Check

Grep the entire profile directory for concrete values that should have been
reverse-templated. Get the concrete values from the team repo's member
manifests.

```bash
PROFILE_DIR="profiles/<name>"

# Member directory names
grep -rn "<member-dir-1>" "$PROFILE_DIR" && echo "FAIL" || echo "PASS"
grep -rn "<member-dir-2>" "$PROFILE_DIR" && echo "FAIL" || echo "PASS"

# Team/org/project names
grep -rn "<team-name>" "$PROFILE_DIR" && echo "FAIL" || echo "PASS"
grep -rn "<org-name>" "$PROFILE_DIR" && echo "FAIL" || echo "PASS"
```

Any match is a FAILURE.

### 2. YAML Validation

For every ralph.yml and botminter.yml modified:
```bash
python3 -c "import yaml; yaml.safe_load(open('$FILE'))"
```

### 3. BM Marker Integrity

Verify all BM injection markers are properly paired:
```bash
for f in $(grep -rl "BM:" "$PROFILE_DIR"); do
  opens=$(grep -c "<!-- BM:" "$f")
  closes=$(grep -c "<!-- /BM:" "$f")
  [ "$opens" = "$closes" ] || echo "FAIL: $f ($opens opens, $closes closes)"
done
```

### 4. Agent Tag Integrity

Verify agent-conditional tags are properly paired:
```bash
for f in $(grep -rl "+agent:" "$PROFILE_DIR"); do
  opens=$(grep -c "+agent:" "$f")
  closes=$(grep -cE '^\s*(<!--|#)\s*-agent' "$f")
  [ "$opens" = "$closes" ] || echo "FAIL: $f"
done
```

### 5. Template Placeholder Presence

Verify `{{member_dir}}` exists in role ralph.yml files:
```bash
for ralph in "$PROFILE_DIR"/roles/*/ralph.yml; do
  grep -q "{{member_dir}}" "$ralph" || echo "FAIL: $ralph missing {{member_dir}}"
done
```

### 6. Cross-Role Consistency

Verify shared skills are identical across roles:
```bash
for skill in member-tuning process-evolution retrospective role-management team-design; do
  diff "$PROFILE_DIR/roles/engineer/coding-agent/skills/$skill/SKILL.md" \
       "$PROFILE_DIR/roles/chief-of-staff/coding-agent/skills/$skill/SKILL.md" \
    || echo "FAIL: $skill differs between roles"
done
```

### 7. Diff Review

Generate the complete diff:
```bash
git diff -- "profiles/<name>/"
```

Walk through every hunk. Each must be an intentional backport.

### 8. Build and Test (HARD GATE)

Profile changes are embedded at compile time. Changes MUST pass:
```bash
just clippy && just test
```

This includes unit tests, conformance tests, and e2e tests. If tests fail,
fix before committing. See `invariants/profiles_updated.md` and
`invariants/no-hardcoded-profiles.md`.

## Troubleshooting

### Profile Source Not Found

Check that `profiles/` exists at the CWD root. If not, you are not in the
botminter project directory.

### Team Repo Path Invalid

The `team_repo_path` must point to a directory containing `botminter.yml`,
`members/`, `coding-agent/`, and `PROCESS.md`. If these are missing, the
path may be pointing to a workspace root instead of the team repo within it.

### Reverse-Templating Misses Concrete Values

Check the member-to-role mapping — a member may have been missed. Read
ALL `members/*/botminter.yml` files to get the full list of concrete values.
Also check for non-standard member names added after `bm init`.

### ralph.yml YAML Validation Fails

`{{member_dir}}` inside YAML values without quotes is valid YAML (they are
raw text in instruction strings). If the failure is in a YAML key or
structure, the reverse-templating corrupted the file — inspect and fix.

### Cross-Role Consistency Failures

Usually means changes from one member's copy were applied without updating
other roles' copies. Copy the correct version to all applicable role
directories and re-check.

### Tests Fail After Profile Changes

Profile changes affect the compiled binary. Common failures:
- Conformance tests asserting specific file presence or content
- E2E tests asserting specific profile behavior during `bm init`
- Unit tests with expected output for context rendering

Fix the tests to match the new profile content, or fix the profile content
if the tests caught a real issue.
