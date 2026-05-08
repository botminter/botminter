# Architectural Decision Records

## Overview

This skill creates and manages Architectural Decision Records (ADRs) with sequential `ADR-NNNN` IDs. ADRs are formal, immutable documents that capture significant architectural decisions — why a decision was made, what alternatives were considered, and what consequences follow.

ADRs are referenced by the PDD skill during design (when `D-NN` decisions are made) and by the code-task-generator skill (when task decomposition surfaces architectural choices). The `D-NN` decision in a design document is the lightweight inline record; the ADR is the full formal document with context, decision rationale, and consequences.

## Parameters

- **title** (required): Short title of the architectural decision (e.g., "Use Redis for session storage", "Adopt event-sourcing for audit trail")
- **context** (optional): Background context, problem statement, or link to the design doc `D-NN` decision that triggered this ADR. If invoked from within a PDD session, the `D-NN` decision context is automatically provided.
- **adr_dir** (optional, default: `team/specs/adrs/`): Directory where ADR files are stored. ADR numbering is global (team-wide) — all ADRs across all projects share the same sequence.

**Constraints for parameter acquisition:**
- You MUST ask for all required parameters upfront
- If `title` is not provided, you MUST ask for it before proceeding
- If `context` is not provided, you SHOULD infer context from the current session (e.g., the `D-NN` decision being recorded, the task decomposition that surfaced the decision)

## Mode Behavior

This skill supports two runtime modes, determined by the context in which it is invoked:

| Mode | Trigger | Human Present | Behavior |
|------|---------|---------------|----------|
| **Interactive** | Invoked directly or from interactive PDD/code-task-generator session | Yes | Present proposed ADR for review, solicit feedback on decision framing and consequences |
| **Auto** | Invoked from autonomous PDD or code-task-generator session | No | Generate ADR autonomously, document reasoning inline, proceed without confirmation |

**Interactive mode rules:**
- Present the proposed ADR content for user review before writing the file
- Ask for feedback on decision framing, alternatives, and consequences
- Allow the user to refine the ADR before finalizing

**Auto mode rules:**
- Generate the ADR autonomously using available context
- Document reasoning for how the decision was framed
- Proceed without confirmation — the ADR is committed as part of the parent skill's commit-after-phase

## Steps

### 1. Determine Next ADR ID

Scan the ADR directory for existing ADRs and assign the next sequential ID.

**Constraints:**
- You MUST scan `{adr_dir}` for existing `ADR-NNNN-*.md` files
- You MUST extract the highest existing ADR number and increment by 1
- If no ADRs exist, start at `ADR-0001`
- ADR numbering is global (team-wide) — all ADRs share one sequence regardless of which project or skill triggered them
- You MUST zero-pad the number to 4 digits (e.g., `ADR-0001`, `ADR-0042`)

### 2. Compose ADR Content

Write the ADR document following the standard format.

**Constraints:**
- You MUST create the ADR file at `{adr_dir}/ADR-NNNN-<title-slug>.md`
- The title slug MUST be lowercase, hyphen-separated, derived from the title parameter (e.g., "Use Redis for Sessions" → `use-redis-for-sessions`)
- You MUST use the exact format specified in the ADR Format section below
- You MUST set the initial status to `Proposed` unless the decision has already been accepted (e.g., when generating from a finalized design document, use `Accepted`)
- If invoked from a PDD session with a `D-NN` decision, you MUST include a reference linking back to the design document and the specific `D-NN` entry
- If invoked from code-task-generator, you MUST include a reference to the story and task context where the decision emerged
- You MUST document both positive and negative consequences — ADRs that list only benefits are incomplete

**Constraints (interactive mode):**
- You MUST present the composed ADR to the user for review before writing the file
- You MUST allow the user to refine any section

**Constraints (auto mode):**
- You MUST generate the complete ADR autonomously
- You MUST infer consequences from the decision context, available research, and codebase knowledge

### 3. Write ADR File

Persist the ADR to the filesystem.

**Constraints:**
- You MUST create the `{adr_dir}` directory if it does not exist
- You MUST write the ADR file at the path determined in Step 1
- You MUST NOT overwrite an existing ADR file — ADRs are immutable
- If the file already exists (e.g., on retry), you MUST skip creation and report that the ADR already exists

### 4. Report Result

Inform the caller about the generated ADR.

**Constraints:**
- You MUST report the ADR ID (`ADR-NNNN`), file path, and title
- If invoked from PDD, you MUST return the ADR ID so it can be referenced in the design document alongside the `D-NN` decision
- If invoked from code-task-generator, you MUST return the ADR ID so it can be referenced in the catalog README

## ADR Format

Every ADR MUST follow this format exactly:

````markdown
# ADR-NNNN: [Decision Title]

## Status

[Proposed | Accepted | Deprecated | Superseded by ADR-NNNN]

## Context

[Why this decision is needed. Describe the problem, the forces at play, and any constraints.
If this ADR originates from a D-NN design decision, reference it here.]

## Decision

[What was decided. State the decision clearly and concisely.
Include the chosen option and why it was selected over alternatives.]

## Alternatives Considered

[List each alternative with a brief assessment of why it was not chosen.]

1. **[Alternative A]** — [Why not chosen]
2. **[Alternative B]** — [Why not chosen]

## Consequences

### Positive

- [Benefit 1]
- [Benefit 2]

### Negative

- [Trade-off or risk 1]
- [Trade-off or risk 2]

## References

- [Link to design doc D-NN decision, if applicable]
- [Link to requirements CATEGORY-NN, if applicable]
- [Link to related ADRs, if applicable]
- [Link to research R-NN, if applicable]
````

## ADR Lifecycle

ADRs are **immutable records** — they are never edited after acceptance. The lifecycle is:

1. **Proposed** — Initial state when created. The decision is under consideration.
2. **Accepted** — The decision has been approved (by human review or autonomous pipeline completion). Once accepted, the ADR content MUST NOT be modified.
3. **Deprecated** — The decision is no longer relevant (e.g., the feature was removed). Add a note explaining why.
4. **Superseded** — A newer ADR replaces this one. The superseded ADR's status MUST be updated to `Superseded by ADR-NNNN` (this is the ONLY permitted modification to an accepted ADR). The new ADR MUST reference its predecessor.

**Superseding an ADR:**
- To supersede an existing ADR, create a new ADR with a reference to the predecessor
- Update the predecessor's Status line to `Superseded by ADR-NNNN` — this is the only change permitted to an existing ADR
- The new ADR MUST include a reference to the superseded ADR in its References section

## Troubleshooting

### No ADRs Exist Yet
If `{adr_dir}` is empty or does not exist, this is the first ADR. Start numbering at `ADR-0001` and create the directory.

### Invoked Without Context
If no `D-NN` reference or task context is available, the ADR is standalone. This is valid — not all architectural decisions originate from PDD or code-task-generator. Ensure the Context section clearly states why the decision is being recorded.

### Numbering Gaps
ADR numbering MAY have gaps (e.g., `ADR-0001`, `ADR-0003` if `ADR-0002` was created in a different branch). Always use the next number after the highest existing ID — do not fill gaps.

### Concurrent ADR Creation
If multiple skills attempt to create ADRs simultaneously, numbering conflicts may occur. Each skill MUST re-scan the directory immediately before writing to minimize race conditions. If a conflict is detected (file already exists), increment and retry.
