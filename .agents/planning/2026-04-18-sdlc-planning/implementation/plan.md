# Implementation Plan: SDLC Planning & Acceptance Redesign

## Implementation Checklist

- [x] STEP-01: Profile Foundation — Status Graph, PROCESS.md, Artifact Conventions
- [x] STEP-02: PDD Skill — ID System, Standalone Requirements, Flat Structure
- [x] STEP-03: PDD Skill — Runtime Awareness and Commit-After-Phase
- [x] STEP-04: Adversarial Review System
- [x] STEP-05: PDD Skill — Traceability Matrix, Skill Chaining, Scope Detection
- [x] STEP-06: code-task-generator Enhancements
- [x] STEP-07: ADR Skill and PDD ADR Integration
- [x] STEP-08: Hat Wiring, po_gate, and Board Scanner Dispatch
- [x] STEP-09: CLI Extension Mechanism and `bm plan`
- [x] STEP-10: Verification Skill and `bm verify`
- [ ] STEP-11: Backward Generation Skill
- [ ] STEP-12: Bug Handling — Simple and Complex Paths
- [ ] STEP-13: End-to-End Integration and Artifact Index

---

## STEP-01: Profile Foundation — Status Graph, PROCESS.md, Artifact Conventions

**Objective:** Create the `agentic-sdlc-planning` profile as a fork of `agentic-sdlc-minimal` with the new status graph, PROCESS.md lifecycle documentation, and `team/specs/` artifact storage conventions. No skills or hats yet — this is the structural foundation that all subsequent steps build into.

**Requirements:** WIM-01, WIM-03, ART-01

**Acceptance Criteria:** AC-26, AC-27, AC-28, AC-29, AC-31 (partial — profile structure only, skills wired in later steps)

**Implementation Guidance:**

- Fork the `agentic-sdlc-minimal` profile directory to `agentic-sdlc-planning`.
- Replace the status graph definitions with the new statuses from design Section 8.1:
  - Epic lifecycle: 8 statuses (`human:po:triage` → `human:po:backlog` → `eng:lead:plan` → `human:po:plan-review` → `eng:lead:breakdown` → `eng:lead:monitor` → `human:po:accept` → `done`)
  - Story lifecycle: 7 statuses (`eng:lead:plan` → `human:po:plan-review` → `eng:dev:implement` → `eng:qe:verify` → `snt:gate:merge` → `human:po:accept` → `done`)
  - Bug lifecycle: 4 statuses (`human:po:triage` → `eng:qe:investigate` → `eng:qe:monitor` → `done`)
- Rewrite PROCESS.md to document the new lifecycle, issue types (Epic, Story, Task, Bug), entry-at-any-level principle, and two-touch-point contract.
- Add the `team/specs/` directory convention and empty `team/specs/index.md` template to the profile's team repo scaffold.
- Remove the `error` status — failed processing is handled by hat-level error recovery (D-10).
- Add label definitions to the profile: `plan:auto`, `accept:auto`, `tasks:inline`, `tasks:off`, `agent-internal`, `bug:simple`, `bug:complex`, `planning:backward-generate`.

**Test Requirements:**

- `bm init --profile agentic-sdlc-planning` extracts a valid profile with correct directory structure.
- The extracted team repo contains PROCESS.md with the new lifecycle documentation.
- The GitHub project board (when synced) has the correct status field values matching the new graph.
- `team/specs/index.md` exists in the extracted profile with the correct template format.
- Status transition validation: verify that the allowed transitions match the design's state diagrams.

**Integration:** This is the foundation step. No prior work to integrate with.

**Demo:** Run `bm init --profile agentic-sdlc-planning`, then `bm teams sync -a`. Show: the profile extracts cleanly, the team repo has the new PROCESS.md, the board has the correct statuses, and `team/specs/index.md` exists.

---

## STEP-02: PDD Skill — ID System, Standalone Requirements, Flat Structure

**Objective:** Enhance the PDD skill with the ID system (Q-NN, CATEGORY-NN, R-NN, AC-NN, D-NN, STEP-NN), extract requirements into a standalone `requirements.md`, flatten the output directory structure, and remove tool-specific references. These are the core artifact format changes that all downstream traceability depends on.

**Requirements:** ART-03, ART-04 (partial), PLN-03

**Acceptance Criteria:** AC-08, AC-09 (partial — matrix structure only, chaining in STEP-05)

**Implementation Guidance:**

- Design Section 7.1, changes #1, #2, #3, #10.
- **ID system (change #1):** Add ID assignment to each PDD step:
  - Step 3 (idea-honing): tag each question with `Q-NN`
  - New step between 5 and 6: extract requirements into `requirements.md` with `CATEGORY-NN` IDs. Category: 3-5 uppercase chars abbreviated from the section heading. Number: zero-padded, sequential within category.
  - Step 4 (research): tag each topic with `R-NN`
  - Step 6 (design): tag acceptance criteria with `AC-NN`, decisions with `D-NN`. Design doc references requirements by `CATEGORY-NN` instead of duplicating them.
  - Step 7 (plan): tag each step with `STEP-NN`
- **Standalone requirements.md (change #2):** New step after iteration checkpoint. Reads idea-honing answers and extracts requirement statements into categorized table format per the schema in design Section 9.2.
- **Flat structure (change #3):** Output `design.md` and `plan.md` as flat files in the project directory, not `design/detailed-design.md` and `implementation/plan.md`.
- **Tool-agnostic (change #10):** Remove the Kiro-specific `/context add` reference in Step 1. Replace with tool-agnostic language.
- Follow the ID format specification in design Section 9.1.

**Test Requirements:**

- Run PDD with a test rough idea. Verify:
  - Every question in idea-honing.md has a `Q-NN` tag.
  - `requirements.md` exists as a standalone file with `CATEGORY-NN` IDs.
  - Research files have `R-NN` tags.
  - Design doc has `AC-NN` on acceptance criteria, `D-NN` on decisions.
  - Design doc references requirements by `CATEGORY-NN`, does not duplicate requirement text.
  - Plan has `STEP-NN` on each step.
  - Output structure is flat (`design.md`, `plan.md` — not in subdirectories).
  - No Kiro-specific references remain.
- Verify ID numbering: sequential within scope, zero-padded, no gaps.

**Integration:** Built on the profile from STEP-01. The PDD skill is enhanced in-place — the existing SOP file is modified, not replaced.

**Demo:** Run PDD interactively with a small rough idea. Show: the produced artifacts with IDs on all entities, standalone `requirements.md` with categorized IDs, design doc referencing requirements by ID, flat file structure.

---

## STEP-03: PDD Skill — Runtime Awareness and Commit-After-Phase

**Objective:** Add interactive/auto mode detection to PDD so it can run both conversationally (human present) and autonomously (Ralph loop). Add commit-after-each-phase behavior for crash resilience and resumability.

**Requirements:** PLN-01, PLN-02

**Acceptance Criteria:** AC-01 (partial — artifact production in auto mode), AC-03 (interactive session starts)

**Implementation Guidance:**

- Design Section 7.0 (mode behavior) and Section 7.1, changes #4 and #12.
- **Mode detection (change #4):** Add mode detection at skill entry — detect whether a human is present (interactive) or the skill is invoked autonomously. Apply the mode behavior pattern from Section 7.0:
  - Interactive: ask questions, wait for answers, present options, solicit feedback.
  - Auto: self-answer questions using available context (epic body, codebase, team knowledge). Record self-answers in idea-honing.md clearly marked as agent-derived. Skip all human confirmation prompts.
  - The upstream PDD SOP's Step 3 constraints ("MUST ask ONE question at a time," "MUST wait for user's response") apply in interactive mode only. In auto mode, these are relaxed.
- **Commit-after-phase (change #12):** After each completed phase (idea-honing, requirements, research, design, plan), commit artifacts to `team/specs/`. On failure and retry, detect which artifacts already exist and resume from the next phase.

**Test Requirements:**

- **Interactive mode:** Run PDD in a `bm plan` session. Verify: questions asked one at a time, waits for human response, presents options.
- **Auto mode:** Invoke PDD from a Ralph hat with an epic body as input. Verify: self-answers all questions, produces all artifacts without blocking, marks auto-generated answers.
- **Commit behavior:** Interrupt PDD mid-research. Restart. Verify: idea-honing and requirements artifacts exist from previous run, skill resumes from research phase.
- **Resumability:** Verify that the skill correctly detects "requirements.md already exists" and skips the requirements phase.

**Integration:** Builds on STEP-02's ID-enhanced PDD. The mode detection wraps the existing step logic — each step checks the mode and adjusts behavior.

**Demo:** Run PDD in auto mode against a test epic body. Show: the skill self-answers questions in idea-honing.md (clearly marked), produces all artifacts autonomously, commits after each phase. Then simulate a crash mid-run: show that restarting picks up from the last committed phase.

---

## STEP-04: Adversarial Review System

**Objective:** Add the adversarial review mechanism: after each major artifact (requirements, design, plan), spawn 3 reviewer agents with distinct per-artifact-type perspectives. Ship the `lead_plan-review` hat with the zero-trust quality gate instructions.

**Requirements:** REV-01, REV-02, REV-03

**Acceptance Criteria:** AC-05, AC-06, AC-07

**Implementation Guidance:**

- Design Section 4.5 (adversarial review system) and Section 7.1, change #5.
- **Reviewer spawning:** After each major artifact is produced (requirements.md, design.md, plan.md), spawn 3 adversarial reviewer agents in parallel using the coding agent's sub-agent capability.
- **Per-artifact perspectives (D-05):** Use the perspective table from Section 4.5:
  - Requirements: completeness, feasibility, testability
  - Design doc: architecture, security, maintainability
  - Plan: scope, dependency correctness, risk
  - Acceptance criteria: coverage, observability, edge cases
- **Feedback format:** Each reviewer produces structured feedback with verdict (PASS/REVISE/BLOCK), severity-tagged issues, and concrete suggestions — per the format in Section 4.5.
- **Mode-dependent behavior:**
  - Interactive: present consolidated feedback to the human. Human selectively addresses issues ("fix #1 and #3, skip #2"). PDD revises only addressed items.
  - Autonomous: iterate up to 3 rounds. Round 1: initial review. Round 2: targeted revision of blocker/major only. Round 3: final pass — if blockers remain, emit rejection event.
- **`lead_plan-review` hat:** Create the hat configuration with the zero-trust quality gate instructions from Section 5.2 (the full hat prompt). This is an internal hat within the `eng:lead:plan` status — it activates after `lead_plan-create` completes. The hat's instructions are shipped with the profile.

**Test Requirements:**

- Produce a design doc with a known hallucinated reference (e.g., reference a non-existent API). Verify at least one reviewer catches it.
- Produce requirements with a hollow requirement ("system MUST be robust"). Verify the testability reviewer flags it.
- In interactive mode: verify that rejected issues are presented and the human can selectively dismiss.
- In autonomous mode: verify max 3 iterations, verify rejection event emitted with remaining issues after round 3.
- Verify that the `lead_plan-review` hat activates after `lead_plan-create` within the `eng:lead:plan` status.

**Integration:** Builds on STEP-03's mode-aware PDD. The review step is inserted after artifact production in PDD Steps 6 and 7. The `lead_plan-review` hat is added to the profile's ralph.yml.

**Demo:** Run PDD and let it produce a design doc. Show: 3 reviewers spawn with distinct perspectives, feedback is presented in structured format with verdicts and severity. In interactive mode, show selective acceptance of feedback. In auto mode, show the iteration loop.

---

## STEP-05: PDD Skill — Traceability Matrix, Skill Chaining, Scope Detection

**Objective:** Complete the PDD skill enhancements: add the traceability matrix to the design doc, make plan steps map 1:1 to stories, add skill chaining to code-task-generator in interactive mode, and add downward scope detection.

**Requirements:** ART-04, PLN-07 (partial — downward scope detection)

**Acceptance Criteria:** AC-09, AC-10, AC-14, AC-23

**Implementation Guidance:**

- Design Section 7.1, changes #6, #7, #8, #11.
- **Traceability matrix (change #8):** Add a traceability matrix at the end of the design doc: requirement (`CATEGORY-NN`) → acceptance criterion (`AC-NN`) → implementation step (`STEP-NN`) → verification status (initially "Pending"). Every requirement MUST appear in the matrix mapped to at least one AC and step.
- **Plan steps = stories (change #7):** Each step in plan.md maps 1:1 to a story. Same content structure (objective, guidance, test requirements, integration, demo) but explicitly story-shaped — the format is directly usable by the breakdown hat to create GitHub issues without transformation.
- **Skill chaining (change #6):** After plan is complete in interactive mode: ask the user whether to create story issues from plan steps, and whether to chain into code-task-generator for task decomposition per story. User chooses sequencing (all stories first then decompose, or story-by-story). In autonomous mode: no chaining — Ralph hat transitions handle downstream.
- **Downward scope detection (change #11):** During idea-honing (Step 3), detect story-scope signals: clear single objective, no technology unknowns, 1-step plan, no architectural decisions. If detected, offer to switch to code-task-generator (story-level) instead. In interactive: ask the user. In autonomous: demote to story.

**Test Requirements:**

- Produce a design doc. Verify traceability matrix: every `CATEGORY-NN` in requirements.md appears, mapped to at least one `AC-NN` and `STEP-NN`.
- Verify plan steps are story-shaped: each has objective, guidance, test requirements, integration, demo sections.
- In interactive mode after plan: verify the skill asks about story creation and code-task-generator chaining.
- Feed PDD a well-defined, single-objective idea (story-scope). Verify it offers to demote to code-task-generator.
- Feed PDD a vague, multi-component idea. Verify it proceeds with full epic planning.

**Integration:** Builds on STEP-04's review system. The traceability matrix references all IDs from STEP-02. Skill chaining connects to code-task-generator (enhanced in STEP-06). Scope detection is the complement of code-task-generator's upward detection (STEP-06).

**Demo:** Complete a full PDD run: rough idea → idea-honing → requirements → research → design with traceability matrix → review → plan with story-shaped steps. In interactive mode, show: skill asks about story creation, user chooses to chain into code-task-generator for the first story, tasks are decomposed. Also show: start PDD with a simple, well-defined idea, PDD detects story-scope, offers to demote.

---

## STEP-06: code-task-generator Enhancements

**Objective:** Enhance code-task-generator with traceability IDs, catalog README, updated output location, runtime mode awareness, upward scope detection, and commit behavior. Remove code-assist references.

**Requirements:** PLN-05, OPS-01, OPS-02

**Acceptance Criteria:** AC-12, AC-15, AC-17, AC-18, AC-18a, AC-22

**Implementation Guidance:**

- Design Section 7.2, all 9 changes.
- **Traceability IDs (change #1):** Task files carry `CATEGORY-NN` requirement IDs and `AC-NN` acceptance criteria IDs from the parent story/design doc. Add a "Traceability" section per the format in Section 7.2.
- **Catalog README (change #2):** Generate `README.md` in each story's task folder cataloging all tasks: number, title, status, requirement IDs, AC IDs.
- **Remove code-assist (change #3):** Tasks are implemented by the agent runtime (Ralph loops). Remove all code-assist references.
- **Runtime mode (change #4):** Apply Section 7.0 mode behavior. Auto mode skips user approval prompts, documents decisions in the catalog README.
- **Output location (change #5):** Tasks live at `team/specs/<issue#>-<epic-slug>/tasks/<issue#>-<story-slug>/`.
- **Story-aware naming (change #6):** Folder names use `<issue#>-<story-slug>/` instead of `step{NN}/`.
- **Upward scope detection (change #7):** During decomposition, check for epic-scope signals (vague input, research needed, architecture decisions, multi-component, >5 tasks, open questions). If detected, offer to switch to PDD. In interactive: ask user. In autonomous: create epic, link story.
- **Commit (change #8):** Commit task files and catalog README after generation.
- **ADR invocation (change #9):** Deferred to STEP-07 — a stub or no-op is acceptable here.

**Test Requirements:**

- Decompose a story into tasks. Verify:
  - Each `.code-task-NN.md` has a Traceability section with `CATEGORY-NN` and `AC-NN`.
  - `README.md` catalog exists with all tasks listed.
  - Output is at the correct `team/specs/` path with issue-number-based naming.
  - No code-assist references in output or skill text.
- **Externalization modes:** Test with `tasks:inline` label → tasks appear as structured section in story issue. Test with `tasks:off` label → no GitHub issues. Test default (no label) → full issues with `agent-internal` label.
- **Scope detection:** Feed a story that is really epic-scope (vague, multi-component). Verify the skill detects and offers to switch to PDD.
- **Auto mode:** Invoke from a Ralph hat. Verify: no user prompts, decisions documented in README.

**Integration:** Connects to STEP-05's skill chaining — PDD chains into code-task-generator after producing the plan. Uses the ID system from STEP-02 to carry traceability forward. Uses the artifact storage conventions from STEP-01.

**Demo:** Start with a story on the board. Show: code-task-generator decomposes it into tasks with IDs, produces a catalog README, stores everything in `team/specs/`. Then show task externalization: default creates GitHub issues with `agent-internal` label; `tasks:inline` puts them in the story issue; `tasks:off` keeps them repo-only.

---

## STEP-07: ADR Skill and PDD ADR Integration

**Objective:** Create the ADR skill for managing Architectural Decision Records, and wire it into PDD (change #9) and code-task-generator (change #9) so that D-NN decisions produce formal ADR-NNNN documents.

**Requirements:** ART-03

**Acceptance Criteria:** (supports AC-08, AC-09 — decisions are part of the traceability chain)

**Implementation Guidance:**

- Design Section 7.5 (ADR skill specification).
- **ADR skill:** New skill with parameters: `title` (required), `context` (optional), `adr_dir` (optional, default `team/specs/adrs/`).
  1. Assign next sequential `ADR-NNNN` ID — scan existing ADRs in the directory for the highest number.
  2. Create `ADR-NNNN-<title-slug>.md` following standard ADR format: Title, Status (Proposed/Accepted/Deprecated/Superseded), Context, Decision, Consequences (positive + negative), References.
  3. If invoked from PDD, link back to the `D-NN` decision in the design doc.
- **ADR lifecycle:** ADRs are immutable — never edited, only superseded by new ADRs with a link to the predecessor.
- **PDD integration (Section 7.1, change #9):** When PDD produces a `D-NN` decision during design, invoke the ADR skill to generate a formal ADR. The design doc's `D-NN` is the lightweight inline record; the ADR is the full formal document.
- **code-task-generator integration (Section 7.2, change #9):** When decomposition surfaces an architectural decision (e.g., choosing between implementation approaches), invoke the ADR skill.

**Test Requirements:**

- Invoke the ADR skill directly with a title and context. Verify: `ADR-NNNN-<slug>.md` created with correct format, sequential numbering.
- Run PDD through a design that produces D-NN decisions. Verify: each D-NN triggers ADR generation, ADR file links back to the D-NN, design doc references the ADR-NNNN.
- Create two ADRs. Supersede the first. Verify: first ADR's status is "Superseded by ADR-NNNN", second ADR links to predecessor.
- Verify global numbering: ADR IDs are team-wide sequential, not per-project.

**Integration:** Builds on PDD from STEP-02–05 (D-NN decisions already exist in the design doc). Wires into the commit-after-phase behavior from STEP-03.

**Demo:** Run PDD on a design that requires an architectural decision (e.g., "store sessions in Redis vs. database"). Show: the D-NN decision is recorded inline in the design doc, and a full ADR-NNNN document is generated in `team/specs/adrs/` with context, decision, and consequences.

---

## STEP-08: Hat Wiring, po_gate, and Board Scanner Dispatch

**Objective:** Wire all 15 engineer hats into the profile's ralph.yml, implement the `po_gate` hat with auto-advance label support, and update the board scanner to dispatch `human:*` statuses to `po_gate` instead of skipping them.

**Requirements:** WIM-04, PLN-04

**Acceptance Criteria:** AC-01 (complete), AC-02, AC-26, AC-27, AC-28, AC-31 (complete)

**Implementation Guidance:**

- Design Section 5.2 (statuses and hats) and Section 8.3 (board scanner).
- **Hat configuration:** Wire all 15 engineer hats into ralph.yml per the hat table in Section 5.2. Hat naming: `<persona>_<activity>` with hyphen suffix for internal phases (`<persona>_<activity>-<phase>`). Internal hats (`lead_plan-create`, `lead_plan-review`, `dev_implement-red`, `-green`, `-refactor`, `-review`) are not board statuses — they cycle within their parent status.
- **Internal hat cycles:**
  - `eng:lead:plan`: `lead_plan-create` → `lead_plan-review`. Reject → `lead_plan-create` iterates. Pass → `human:po:plan-review`.
  - `eng:dev:implement`: Per task: `dev_implement-red` → `dev_implement-green` → `dev_implement-refactor` → `dev_implement-review`. Reject → back to red. All tasks done → `eng:qe:verify`.
- **`po_gate` hat:** Single hat for all `human:po:*` gates. Per-status branching:
  - `human:po:triage`: always waits for human response.
  - `human:po:backlog`: always waits for human to prioritize.
  - `human:po:plan-review`: check for `plan:auto` label → auto-advance with comment. No label → post review request, poll for human response.
  - `human:po:accept`: check for `accept:auto` label → auto-advance with comment. No label → post review request, poll for human response.
- **Board scanner dispatch (Section 8.3):** Change `human:*` statuses from "skip" to "dispatch `po_gate`." The `po_gate` hat is the gatekeeper, not the scanner.
- **Hat-to-status mapping:** Apply the changes from D-07 (Section 5.2): collapse `po_backlog` + `po_reviewer` into `po_gate`, rename arch hats to lead, split developer into TDD phases, remove test designer, remove bug-specific hats.

**Test Requirements:**

- Place an epic at `eng:lead:plan`. Verify: `lead_plan-create` hat activates.
- After `lead_plan-create` completes, verify: `lead_plan-review` hat activates (internal transition).
- After `lead_plan-review` passes, verify: transitions to `human:po:plan-review`.
- At `human:po:plan-review` with `plan:auto` label: verify auto-advance to `eng:lead:breakdown` with comment.
- At `human:po:plan-review` without `plan:auto`: verify review request posted, waits for human.
- At `human:po:accept` with `accept:auto`: verify auto-advance to `done` with comment.
- Place a story at `eng:dev:implement`. Verify: TDD hat cycle per task (`red` → `green` → `refactor` → `review`).
- Board scanner: verify `human:po:*` statuses are dispatched to `po_gate`, not skipped.

**Integration:** Connects the profile from STEP-01 (statuses), the PDD skill from STEP-02–05 (`lead_plan-create` invokes PDD), the adversarial review from STEP-04 (`lead_plan-review` runs quality checks), and code-task-generator from STEP-06 (`lead_plan-create` invokes code-task-generator for stories).

**Demo:** Place an epic on the board at `human:po:triage`. Walk through the full status lifecycle: triage (human approves) → backlog → plan (PDD runs, review runs) → plan-review (show both `plan:auto` auto-advance and default human-wait paths) → breakdown → monitor. Show the board scanner dispatching `human:*` statuses to `po_gate`.

---

## STEP-09: CLI Extension Mechanism and `bm plan`

**Objective:** Build the manifest-driven CLI extension mechanism in the `bm` binary, and ship `bm plan` as the first extension — a collaborative planning session with the engineer wearing the `lead_plan-create` hat.

**Requirements:** PLN-01 (interactive sessions via CLI)

**Acceptance Criteria:** AC-03, AC-04, AC-32

**Implementation Guidance:**

- Design Sections 8.2.1 (extension mechanism) and 8.2.2 (`bm plan`). Research R-08.
- **Extension mechanism (4 files changed, 1 new):**
  1. `profile/manifest.rs` — Add `extensions: Vec<Extension>` to `ProfileManifest`. Define `Extension` struct (name, description, member role, hat, args) and `ExtensionArg` struct (name, positional/long, type, required, description).
  2. `cli.rs` — Add `#[command(external_subcommand)] External(Vec<OsString>)` variant to the `Command` enum. This catches any subcommand not in the static tree.
  3. `main.rs` — Dispatch arm for `External`: detect active workspace (walk CWD for `.botminter.workspace`), read manifest, find matching extension, resolve member from role, call `chat::prepare_chat_session()`.
  4. `commands/extension.rs` (new) — Generic extension dispatch: resolve member from role, validate hat exists in ralph.yml, map CLI args to initial prompt or context, call existing chat session pathway.
  5. `commands/completions.rs` — Extend `build_cli_with_completions()` to inject extension subcommands from the active workspace's manifest.
- **Workspace detection at startup:** Walk up from CWD to find `.botminter.workspace` marker. If found, read the team's `botminter.yml` for extensions. If not found (or no extensions field), `External` variant produces a "unknown command" error.
- **`bm plan` extension in the profile manifest:**
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
  ```
- **Entry points:**
  - `bm plan "I want to add OAuth support"` — rough idea as positional arg, becomes initial prompt.
  - `bm plan --epic 42` — load epic issue body as the rough idea input.
  - `bm plan` (no args) — start session, agent prompts for input.

**Test Requirements:**

- **Extension mechanism:** Add a test extension to a profile manifest. Verify: `bm <name>` is available inside that workspace, not available outside any workspace, not available in a workspace with a different profile.
- **Help text:** `bm --help` inside the workspace lists `plan` with its description. Outside the workspace, `plan` does not appear.
- **Shell completions:** Tab-completing `bm pl` inside the workspace completes to `bm plan`.
- **`bm plan`:** Starts a session with the engineer in `lead_plan-create` hat. PDD begins interactively.
- **`bm plan --epic 42`:** Loads the epic body from GitHub and uses it as the rough idea.
- **`bm plan "test idea"`:** Rough idea forwarded as the initial prompt.
- **Isolation:** Extension dispatch calls `chat::prepare_chat_session()` — verify the session is identical to `bm chat engineer-bob --hat lead_plan-create`.

**Integration:** Depends on the PDD skill (STEP-02–05), hat wiring (STEP-08), and profile structure (STEP-01). The extension mechanism is a BotMinter core change — it modifies the `bm` binary, not just the profile.

**Demo:** From inside a workspace with the `agentic-sdlc-planning` profile: run `bm --help` and show `plan` in the command list. Run `bm plan "I want to add user roles and permissions"` — show the engineer starts in the `lead_plan-create` hat, PDD begins interactively. Then step outside the workspace and run `bm plan` — show the command is not available.

---

## STEP-10: Verification Skill and `bm verify`

**Objective:** Create the conversational verification skill that walks the user through acceptance criteria, captures gaps, and produces `verification.md`. Ship the `bm verify` extension in the profile manifest.

**Requirements:** VER-01, VER-02, VER-03

**Acceptance Criteria:** AC-19, AC-20, AC-21

**Implementation Guidance:**

- Design Sections 6.1, 6.2, 7.6, and 8.2.2 (`bm verify`).
- **Verification skill (Section 7.6):**
  1. Locate planning artifacts for the work item via three discovery paths: workspace convention (`team/specs/<issue#>-<slug>/`), team repo index (`team/specs/index.md`), issue body links.
  2. Load acceptance criteria (`AC-NN`) from the design doc or story.
  3. Present each criterion one at a time: show the GWT text, ask the user for their assessment.
  4. Record per-criterion: PASS, SKIP, or FAIL with user's natural language.
  5. Auto-infer gap severity from the user's language (D-09): "crashes" → blocker, "slow" → minor, "doesn't work" → major. Never ask the user to classify severity explicitly.
  6. Generate `verification.md` in `team/specs/<issue#>-<slug>/` per the format in Section 6.2: results table, gap details with `GAP-NN` IDs, summary stats.
  7. Update `team/specs/index.md`: work item status → `verified`.
- **`bm verify` extension in the profile manifest:**
  ```yaml
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
- **Interactive-only:** This skill requires a human to assess criteria. It is never invoked autonomously.

**Test Requirements:**

- Invoke `bm verify 87` on a story with planning artifacts. Verify: criteria loaded, presented one at a time.
- Respond "works" to one criterion and "crashes when I click submit" to another. Verify: first recorded as PASS, second recorded as FAIL with severity "blocker" auto-inferred.
- Respond "skip" to a criterion. Verify: recorded as SKIP.
- After session ends: verify `verification.md` exists with correct results table, GAP-NN entries, and summary stats.
- Verify `team/specs/index.md` updated to status `verified`.
- Verify artifact discovery works via all three paths.

**Integration:** Depends on the CLI extension mechanism from STEP-09. Depends on planning artifacts produced by PDD (STEP-02–05) and stored in `team/specs/` (STEP-01). Uses the AC-NN IDs from the design doc. Integrates with the `human:po:accept` status gate from STEP-08 — verification happens before acceptance.

**Demo:** Run `bm verify 87` against a story with known acceptance criteria. Walk through criteria: pass some, fail some (show severity inference from natural language), skip one. Show the generated `verification.md` with results table and gap entries. Show the index update.

---

## STEP-11: Backward Generation Skill

**Objective:** Create the composite backward-generation skill that produces upstream planning artifacts retroactively for work implemented without epic-level planning.

**Requirements:** PLN-06, PLN-07 (backward generation trigger)

**Acceptance Criteria:** AC-24, AC-25

**Implementation Guidance:**

- Design Section 7.4 (backward-generation skill specification).
- **Composite skill:** Chains codebase-summary then synthesis.
  1. Run codebase-summary against the project codebase. Produces architecture.md, components.md, interfaces.md, data_models.md, workflows.md.
  2. Synthesize `requirements.md` from the codebase analysis: derive capabilities from code behavior, test assertions, API contracts. Each capability becomes a requirement with `CATEGORY-NN` ID.
  3. Synthesize `design.md` from the architecture analysis: formalize structure into a design doc with `AC-NN` acceptance criteria (derived from tests/behaviors) and `D-NN` decisions (inferred from patterns).
  4. Synthesize `plan.md` from implementation: existing stories/tasks become plan steps retrospectively.
  5. Store artifacts in `team/specs/<issue#>-<slug>/`. Update `team/specs/index.md` with status `done`.
- **Not PDD in reverse:** No idea-honing (code IS the answer), no research (technology already chosen), no iterative Q&A. The agent reads code and produces documentation.
- **Trigger:** Autonomous mode: runs only when `planning:backward-generate` label is present. Interactive mode: operator invokes directly.
- **Scope:** Story → epic level only. No retroactive task decomposition (operational files consumed during execution add no archival value).

**Test Requirements:**

- Run backward generation against a project with implemented code and tests. Verify: `requirements.md` produced with IDs derived from code behavior, `design.md` with AC derived from tests, `plan.md` with retrospective steps.
- Story with `planning:backward-generate` label: verify skill triggers after story completion.
- Story without the label: verify no backward generation occurs (AC-25).
- Verify all produced artifacts follow the same format and ID conventions as forward-planned artifacts.
- Verify `team/specs/index.md` updated.

**Integration:** Uses codebase-summary (STEP-01 confirms minimal changes needed). Follows the same artifact storage conventions from STEP-01. Uses the same ID system from STEP-02. Connects to the specs index from STEP-01.

**Demo:** Take a project with several implemented stories that have no epic-level planning artifacts. Add the `planning:backward-generate` label to one story. Show: backward generation runs, produces requirements.md (capabilities derived from code), design.md (architecture from codebase analysis, AC from tests), plan.md (retrospective). Compare the retroactively generated artifacts to what forward planning would have produced.

---

## STEP-12: Bug Handling — Simple and Complex Paths

**Objective:** Implement the bug lifecycle with two tracks: simple bugs that create stories with `plan:auto`, and complex bugs that create stories with full human gates. Add the `qe_investigate` and `qe_monitor` hats.

**Requirements:** WIM-05

**Acceptance Criteria:** AC-33, AC-34, AC-35, AC-36

**Implementation Guidance:**

- Design Section 4.4 (bug handling) and Section 8.1 (bug lifecycle).
- **Bug lifecycle:** `human:po:triage` → `eng:qe:investigate` → `eng:qe:monitor` → `done`.
- **`qe_investigate` hat:**
  - Picks up triaged bugs.
  - Reproduces and confirms the bug.
  - Determines simple vs. complex per D-12:
    - `bug:simple` label → treat as simple without analysis.
    - `bug:complex` label → immediately create story without attempting fix.
    - No label → attempt fix. If fails after 3 attempts, escalate to complex.
    - Agent judgment: if fix spans multiple components or requires design decisions → complex.
  - Creates a linked Story issue:
    - Simple: Story with `plan:auto` label. Planning auto-advances, acceptance human-gated by default.
    - Complex: Story without auto-advance labels — full human-gated flow.
  - Transitions bug to `eng:qe:monitor`.
- **`qe_monitor` hat:** Watches the linked Story. When the Story is done and merged, verifies the fix on the bug, then closes it.
- **Simple bug story:** Developer hat writes regression test + fix. Since `plan:auto` is set, planning auto-advances. Acceptance still human-gated unless user adds `accept:auto`.
- **Complex bug story:** Follows normal story flow with code-task-generator decomposition and human gates.

**Test Requirements:**

- Report a bug with `bug:simple` label. Verify: `qe_investigate` picks it up, creates a Story with `plan:auto`, bug moves to `eng:qe:monitor`. Story is implemented with regression test + fix.
- Report a bug with `bug:complex` label. Verify: `qe_investigate` creates a Story without auto-advance, bug moves to `eng:qe:monitor`. Story follows full human-gated flow.
- Report a bug without labels. Verify: agent attempts fix. If simple fix works → treated as simple. If agent fails after 3 attempts → escalates to complex by creating a story.
- Verify `qe_monitor` watches Story and closes bug when fix is merged.
- Verify regression test is produced for simple bugs.

**Integration:** Uses the status graph from STEP-01 (bug lifecycle statuses), hat wiring from STEP-08 (new `qe_investigate` and `qe_monitor` hats), and story flow from STEP-06 (code-task-generator for complex bugs). Story lifecycle from STEP-08 handles the linked story.

**Demo:** Report two bugs: one simple (login error message wrong), one complex (auth flow needs redesign). Show: simple bug → Story with `plan:auto` → auto-advance → developer implements fix with regression test → acceptance gate. Complex bug → Story without auto-advance → full planning and human review. Show `qe_monitor` watching and closing the bug when the story is done.

---

## STEP-13: End-to-End Integration and Artifact Index

**Objective:** Validate the complete system end-to-end across all work item types and entry levels. Ensure the specs index is maintained correctly across all flows. Fix any integration issues discovered during full lifecycle testing.

**Requirements:** WIM-02, ART-02

**Acceptance Criteria:** AC-11, AC-13, AC-30

**Implementation Guidance:**

- Design Section 8.5 (artifact storage and index maintenance).
- **Specs index maintenance:** Verify all hats and skills update `team/specs/index.md` correctly per the transition table in Section 8.5:
  - `lead_plan-create` → adds row with status `planning`
  - `lead_breakdown` or `dev_implement-red` → status `active`
  - `lead_monitor` or `gate_merge` → status `done`
  - Verification skill → status `verified`
  - Backward generation → adds row with status `done`
- **Multi-path artifact discovery (ART-02):** Verify agents can find planning artifacts through all three paths: workspace convention, team repo index, issue body links. Fix any path that's broken.
- **Entry at any level (WIM-02):**
  - Epic entry → PDD → breakdown → stories → tasks → implement → verify → accept
  - Story entry (no parent epic) → code-task-generator → tasks → implement → verify → accept
  - Task entry (no parent story or epic) → implement directly → done
- **Story-to-epic traceability (AC-11):** Verify that stories created from an epic's plan steps link back to the parent epic's planning artifacts.
- **Cross-feature integration:** Verify that all components work together:
  - Adversarial review + plan:auto → review runs internally even though plan-review auto-advances
  - Backward generation + verification → retroactive artifacts are verifiable
  - Bug escalation → linked story goes through full lifecycle
  - Task externalization modes work with the specs index

**Test Requirements:**

- **Full epic lifecycle (interactive):** `bm plan` → PDD → review → human approval → breakdown → implementation → `bm verify` → human accepts → done. Verify all artifacts produced, all statuses visited, specs index updated at each transition.
- **Full epic lifecycle (autonomous with `plan:auto` + `accept:auto`):** Epic on board → PDD auto-mode → review → auto-advance → breakdown → implementation → auto-advance → done. Verify same artifacts, auto-advance comments logged.
- **Story entry:** Story without epic → code-task-generator → implementation → verification. Verify tasks stored correctly, no parent epic artifacts expected.
- **Task entry:** Direct task → developer implements → done. No decomposition or planning ceremony.
- **Bug → Story → Done:** Simple and complex bug paths through to completion.
- **Specs index accuracy:** After all scenarios, verify `team/specs/index.md` reflects the correct state of all work items.
- **Artifact discovery:** From each of the three paths, verify an agent can locate the same artifacts.

**Integration:** This is the capstone step that validates all previous steps work together. No new components — integration testing and gap resolution only.

**Demo:** Walk through three complete scenarios:
1. **Epic lifecycle:** Create an epic, plan interactively, approve, break down into stories, implement one story with TDD cycle, verify acceptance criteria, accept. Show the specs index and artifact tree.
2. **Story entry:** Create a standalone story, decompose into tasks, implement, verify. Show that no epic-level ceremony was required but artifacts are still produced.
3. **Task entry:** Create a direct task, implement, done. Show the lightweight path with no planning overhead.
