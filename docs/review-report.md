# Documentation Review Report

Three independent review agents evaluated the botminter documentation site (17 pages). This report consolidates their findings.

**Last updated**: 2026-02-21 (post-M3 refresh)

## Structure & Navigation

All checks passed:

| Check | Result |
|-------|--------|
| Navigation depth (max 2 levels) | Pass |
| Orphaned pages | Pass — all 17 files mapped |
| Broken internal links | Pass — ~50 links verified |
| Diataxis classification | Pass |
| Document length (< 3000 words) | Pass — longest is 1,042 words |
| Heading hierarchy | Pass — no skipped levels |
| First paragraph quality | Pass |

## Technical Accuracy

### High impact — factual errors

| ID | File | Issue | Status |
|----|------|-------|--------|
| D5.1 | `reference/configuration.md` | `.botminter.yml` field documented as `emoji` — actual field is `comment_emoji` | ✅ Fixed |
| D5.2 | `reference/configuration.md` | `ralph.yml` field documented as `max_runtime` — actual field is `max_runtime_seconds` | ✅ Fixed |
| D2.1 | `reference/process.md` | Lists 7 story statuses as "Milestone 3" — should be Milestone 4 | ✅ Fixed (updated to M4) |

### Medium impact — incomplete information

| ID | File | Issue | Status |
|----|------|-------|--------|
| D4.4 | `reference/member-roles.md` | Designer backpressure: missing "addresses all applicable invariants" | ✅ Fixed |
| D4.5 | `reference/member-roles.md` | Breakdown executor backpressure: missing "references the parent epic" | ✅ Fixed |
| D1.1 | `reference/cli.md` | `bootstrap-labels` omits `status/lead:*` labels | Open — upstream source issue (D2.2) |

### Low impact — omissions

| ID | File | Issue | Status |
|----|------|-------|--------|
| D3.2 | n/a | `summary.md` says "Copy (not symlink)" but implementation uses symlinks — docs are correct | No action needed |
| D7.1 | `roadmap.md` | M2 status differs between docs and summary.md | ✅ Fixed (summary.md updated) |

### Skipped — historical context not relevant to user-facing docs

| ID | Issue | Reason skipped |
|----|-------|----------------|
| D3.1 | Design docs reference `.github-sim/` — docs don't mention it | Historical implementation detail |
| D3.3 | Design doc skeleton includes `.github-sim/` — docs omit it | Replaced by GitHub in current implementation |
| D2.2 | `status/lead:*` labels absent from PROCESS.md source too | Upstream source issue, not a docs issue |

### Already fixed — ralph.yml alignment

| ID | Issue | Resolution |
|----|-------|------------|
| D5.3 | Docs say `starting_event` must not be set; both ralph.yml files set it | Removed `starting_event` from both ralph.yml files |
| D5.4 | Docs say `LOOP_COMPLETE` must not be in `publishes`; both ralph.yml files include it | Removed from `publishes` and `default_publishes` in both board_scanner hats |

## Post-M3 Refresh (2026-02-21)

The following updates were made after Milestone 3 completion:

### New content added

| File | What changed |
|------|-------------|
| `reference/cli.md` | Added `bm daemon start/stop/status`, `bm knowledge list/show`, `--formation` flag on `bm start` |
| `reference/configuration.md` | Added global config (`~/.botminter/config.yml`), daemon runtime files, formation config, topology file sections |
| `roadmap.md` | Updated M3 deliverables to include daemon, knowledge, and formation features |

### Staleness fixes

| File | What changed |
|------|-------------|
| `specs/master-plan/summary.md` | Updated M3 from "planning in progress" to complete with full deliverable list |
| `specs/master-plan/summary.md` | Updated M4 as "unblocked by M3 completion" and refreshed suggested next steps |
| `reference/process.md` | Changed story statuses from "Milestone 3" to "Milestone 4" (D2.1) |
| `getting-started/index.md` | Removed placeholder URL for bm CLI |

## Voice, Tone & Quality

### Errors — undefined acronyms

Most acronyms are now expanded on first use per document. Verified expansions for:
PO (Product Owner), HIL (Human-in-the-Loop), QE (Quality Engineer), TDD (Test-Driven Development), CWD (Current Working Directory), M1/M2/M3 (Milestone 1/2/3).

### Warnings

| Category | Count | Files | Status |
|----------|-------|-------|--------|
| Long code blocks (>20 lines) | 2 | workspace-model.md (21 lines), configuration.md (36 lines) | Open — minor |
| Abrupt section transitions | 6 | architecture.md, knowledge-invariants.md, process.md, design-principles.md | Open — minor |
| Missing "Related topics" | 0 | All pages now have Related topics sections | ✅ Resolved |
| Admonition opportunities | 5 | manage-knowledge.md, coordination-model.md, process.md, workspace-model.md, cli.md | Open — minor |
| Passive voice in instructions | 8 | coordination-model.md, process.md, knowledge-invariants.md, configuration.md, profiles.md, design-principles.md | Open — minor |

### Suggestions (optional polish)

- Add brief Ralph explanation for first-time readers on index.md
- Add text alternatives near Mermaid diagrams for accessibility
