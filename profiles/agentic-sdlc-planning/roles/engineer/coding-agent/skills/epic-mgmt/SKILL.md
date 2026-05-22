---
name: epic-mgmt
description: >-
  Manages the full epic planning lifecycle using Prompt-Driven Development
  (PDD). Use for idea honing, requirements extraction, requirements planning,
  research, architectural design, adversarial review, story breakdown, feature
  planning, roadmap planning, and epic decomposition into stories. Also
  triggers on "plan an epic", "design a feature", "run PDD", or /epic-mgmt.
  Do NOT use for stories or tasks — use the story-mgmt skill instead.
metadata:
  author: botminter
  version: 1.0.0
  category: planning
---

# Epic Management (PDD)

## Overview

This skill guides you through the process of transforming a rough idea into a detailed design document with a story breakdown and todo list. It follows the Prompt-Driven Development methodology to systematically refine your idea, conduct necessary research, create a comprehensive design, and develop an actionable story breakdown. The process is designed to be iterative, allowing movement between requirements clarification and research as needed.

All catalogable entities produced by this skill receive stable IDs for cross-referencing across the pipeline:

| Entity | Format | Scope | Assigned By |
|--------|--------|-------|-------------|
| Question | `Q-NN` | Per idea-honing session | Sequential in Step 4 |
| Requirement | `CATEGORY-NN` | Per domain file in project requirements catalogue, continues from highest existing ID | Sequential in Step 7. Category: derived from domain filename uppercased (e.g., `kms.md` → `KMS`). Filename MUST be 3-5 lowercase chars. Number: zero-padded. |
| Research Topic | `R-NN` | Per epic | Sequential in Step 5 |
| Acceptance Criterion | `AC-NN` | Per design doc | Sequential in Step 8 |
| Decision | `D-NN` | Per design doc | Sequential in Step 8 |
| Story | `STORY-NN` | Per plan.md | Sequential in Step 9 |

## Mandatory Orientation

**Constraints:**
- You MUST read the epic issue (if one exists) and scan `team/specs/` for existing artifacts BEFORE starting any PDD step
- If `plan.md` exists with STORY-NN entries AND story issues exist as sub-issues of the epic, you MUST NOT run the PDD pipeline — epic planning is already complete
- If epic planning is complete, you MUST load the `story-mgmt` skill and work at story level
- If the user explicitly asked to work on a specific story (e.g., "decompose story-04", "work on story #8"), you MUST load `story-mgmt` immediately — do NOT run PDD
- If the epic has partial artifacts (e.g., design exists but no plan), you MUST proceed with PDD at the appropriate resumability point (Step 2 handles this)
- If no artifacts exist, you MUST proceed with PDD from Step 1

## Parameters

- **rough_idea** (required): The initial concept or idea you want to develop into a detailed design
- **project** (required): Which BotMinter project (code repository) this epic is for. Corresponds to a directory under `projects/` in the workspace
- **epic_name** (optional): A short, descriptive name for the epic. If not provided, will be generated from the rough idea

The artifact directory (`{epic_dir}`) is derived, not user-specified: `team/specs/{project}/{issue#}-{epic_name}/`

The `{issue#}` is the GitHub Epic issue number, created in Step 2.

**Constraints for parameter acquisition:**
- You MUST ask for all required parameters upfront in a single prompt rather than one at a time
- You MUST support multiple input methods including:
  - Direct input: Text provided directly in the conversation
  - File path: Path to a local file containing the rough idea
  - URL: Link to an internal resource (e.g., wiki page, design doc)
  - Other methods: You SHOULD be open to other ways the user might want to provide the idea
- You MUST use appropriate tools to access content based on the input method
- You MUST confirm successful acquisition of all parameters before proceeding
- If epic_name is not provided, You MUST generate a short kebab-case name from the rough idea (e.g., "template-manager", "auth-system")
- You SHOULD save the acquired rough idea to a consistent location for use in subsequent steps
- You MUST NOT overwrite the existing epic directory because this could destroy previous work and cause data loss
- You MUST ask the operator for a different epic_name if the generated default directory already exists and has contents from a previous iteration

## Mode Behavior

This skill supports two runtime modes, determined by the context in which it is invoked:

| Mode | Trigger | Human Present | Behavior |
|------|---------|---------------|----------|
| **Interactive** | Human runs `bm meetings planning` or invokes skill directly | Yes | Ask questions, wait for answers, present options, solicit feedback at each step |
| **Auto** | Ralph loop picks up work item from the board | No | Execute autonomously — self-answer questions from available context, skip confirmation prompts, document all decisions inline |

**Mode detection:** At skill entry, determine the mode from the runtime context. If a human initiated the session (e.g., via `bm meetings planning` or direct conversation), use interactive mode. If the skill was invoked by a Ralph hat processing a board work item, use auto mode.

**Interactive mode rules:**
- Present proposed actions and ask for confirmation before proceeding
- When multiple approaches exist, explain pros/cons and ask for user preference
- Review artifacts and solicit specific feedback before moving forward
- Ask clarifying questions about ambiguous requirements
- Pause at key decision points to explain reasoning
- Adapt to user feedback and preferences

**Auto mode rules:**
- Execute all actions autonomously without user confirmation
- Document all decisions, assumptions, and reasoning directly in the produced artifacts
- When multiple approaches exist, select the most appropriate and document why in the artifact itself
- Self-answer idea-honing questions using available context: epic body, project codebase, team knowledge, existing artifacts
- Mark all self-generated answers clearly as agent-derived (e.g., `**Answer (agent-derived):**`)
- Provide comprehensive summaries at completion

## Resumability

This skill supports crash recovery and resumability. At the start of each run, check for existing artifacts from a previous incomplete run and resume from the next incomplete phase.

**Important:** Resumability handles **interrupted runs** (crash recovery). It does NOT handle **revision re-entry** — when an artifact was completed and reviewed but the human gate rejected it or requested changes. For revision re-entry, see the "External review revision protocol" in the Adversarial Review section. When both conditions are ambiguous, check the issue's status history to determine whether the skill is resuming after an interruption or re-entering after a rejection.

**Phase completion detection:** A phase is considered complete if its primary output artifact exists and is non-empty:

| Phase | Step | Completion Signal |
|-------|------|-------------------|
| Planning setup | 2 | `{epic_dir}/` directory exists with `rough-idea.md` containing `epic_issue:` frontmatter |
| Idea-honing | 4 | `{epic_dir}/idea-honing.md` has Q-NN entries with answers |
| Research | 5 | `{epic_dir}/research/` directory contains R-NN files |
| Requirements | 7 | `{epic_dir}/requirements-manifest.md` exists listing domain files written |
| Design | 8 | `{epic_dir}/design.md` exists with AC-NN entries |
| Plan | 9 | `{epic_dir}/plan.md` exists with STORY-NN entries |

**Constraints:**
- You MUST check for existing artifacts at the start of Step 2 before creating the planning structure
- If artifacts from a previous run exist, you MUST identify the last completed phase and resume from the next phase
- You MUST NOT overwrite existing completed artifacts — they represent committed work from a previous phase
- You MUST inform the user (interactive) or log in the artifacts (auto) which phases are being skipped due to existing artifacts

## Commit After Phase

After completing each major phase, you MUST commit the artifacts in `{epic_dir}/` to version control with a descriptive commit message. This provides crash resilience — if the skill is interrupted, it can resume from the last committed phase.

**Commit points:**
| After Phase | Files to Commit | Commit Message Pattern |
|-------------|-----------------|----------------------|
| Idea-honing (Step 4) | `idea-honing.md` | `docs(specs): complete idea-honing for {epic_name}` |
| Research (Step 5) | `research/*.md` | `docs(specs): complete research for {epic_name}` |
| Requirements (Step 7) | `team/specs/{project}/requirements/{domain}.md`, `{epic_dir}/requirements-manifest.md` | `docs(specs): extract requirements for {epic_name}` |
| Design (Step 8) | `design.md` | `docs(specs): create design for {epic_name}` |
| Story breakdown (Step 9) | `plan.md` | `docs(specs): create story breakdown for {epic_name}` |

**Constraints:**
- You MUST `git add` and `git commit` the phase artifacts after each phase completes
- You MUST NOT include unrelated files in phase commits
- Commit messages MUST follow the project's commit convention
- On resumability detection (Step 2), committed artifacts are the source of truth for phase completion

## Adversarial Review

After each major planning artifact is produced (requirements, design document, story breakdown), you MUST spawn adversarial reviewer sub-agents in parallel using the coding agent's sub-agent capability. Each reviewer adopts a distinct professional persona and reviews the artifact holistically from that viewpoint — they apply their full professional judgment, not a narrow topic checklist. The reviewers are internal to this skill — they are not separate hats.

**Requirements personas (1 reviewer):**

| Persona | Review lens |
|---------|-------------|
| **Product Manager** | Reviews as the stakeholder who will use this requirements document to evaluate feature completeness and sign off on scope. For each requirement, applies the litmus test: "Can I verify this without reading source code? Does this describe WHAT the system does, not HOW it's built?" Flags implementation details, technology choices, engineering practices, and internal plumbing that leaked into requirements. Also checks for missing requirements — user-observable behaviors implied by the idea-honing but not captured. |

**Design Document personas (3 reviewers):**

| Persona | Review lens |
|---------|-------------|
| **Staff Engineer** | Reviews as a senior IC who will own the technical quality of this system long-term. Applies holistic judgment across architecture, security, maintainability, performance, and production-readiness. Connects decisions to their downstream implications — a design choice that's architecturally clean but operationally fragile gets flagged as one issue, not missed because it falls between two isolated topic reviews. **Abstraction-level gate:** flags any section that prescribes implementation details (function names, file paths, step-by-step implementation instructions, internal struct field assignments) instead of describing component responsibilities, contracts, and behavioral guarantees. The litmus test: "Would a different implementation approach require changing this section?" If yes, it's too tightly coupled and must be raised to the design level or pushed down to stories. |
| **UX Engineer** | Acts as the voice of every user persona who will interact with this feature — end users, operators, admins. Evaluates whether users can find it, learn it, use it, and recover from mistakes. Catches documentation gaps (missing sections, thin content, incomplete guides), poor error messages, CLI ergonomics issues, onboarding friction, and discoverability problems. The test: "when this ships, will users succeed without hand-holding?" |
| **QE Engineer** | Reviews as the engineer who will verify this feature works end-to-end. Evaluates whether the design is testable, acceptance criteria are verifiable in Given-When-Then form, edge cases are covered, the test strategy catches regressions, and observability supports debugging failures. Catches untestable requirements, missing test categories, and gaps between ACs and actual verification. |

**Story Breakdown personas (2 reviewers):**

| Persona | Review lens |
|---------|-------------|
| **Staff Engineer** | Reviews as a tech lead who will guide engineers through this implementation. Evaluates whether stories are right-sized for a single PR, dependencies are correctly ordered, technical risks are identified with mitigations, the build sequence avoids orphaned code, and each story compiles and integrates with previous work. |
| **Delivery/PM** | Reviews as someone accountable for delivering this feature with visible progress. Evaluates whether each story delivers demoable value, the ordering enables early stakeholder feedback, scope is controlled (no gold-plating), stories don't bunch all risk at the end, and the breakdown supports predictable velocity. |

**Review feedback format:**

Each reviewer MUST produce structured feedback in this format:

````markdown
### Review: [Persona]

**Verdict:** PASS | REVISE | BLOCK

**Issues:**
1. [SEVERITY: blocker|major|minor] — [description]
   **Location:** [section/requirement/criterion reference]
   **Suggestion:** [concrete fix]

2. ...

**Strengths:** [what's done well — retained across revisions]
````

**Cross-reference validation (all reviewers):** Reviewers MUST check that file references in the artifact use relative markdown link syntax (e.g., `[kms.md](../requirements/kms.md)`), not plain-text paths. Flag any bare backtick paths like `` `team/specs/...` `` that should be clickable links. Reviewers MUST also verify that linked files actually exist on disk.

**Overlap guidance:** Persona-based reviewers may flag the same concern from different angles. This is expected and is a signal of severity — if both the Staff Engineer and QE flag weak acceptance criteria, that issue is more important than one flagged by a single reviewer. In auto mode, overlapping issues are addressed once (not duplicated); in interactive mode, all feedback is presented and the human decides.

**Iteration protocol:**

| Mode | Behavior |
|------|----------|
| **Interactive** | Present consolidated feedback from all reviewers to the human. The human selectively addresses issues (e.g., "fix #1 and #3, skip #2"). Revise only the items the human chooses to address. The human decides when the artifact is ready. |
| **Auto** | Iterate up to 3 rounds without human input. Round 1: initial review — all issues surfaced. Round 2: targeted revision — address only blocker and major issues, re-review changed sections plus regression check. Round 3: final pass — if blockers remain, emit rejection event with remaining issues listed. |

**Constraints:**
- You MUST spawn the number of reviewer sub-agents matching the artifact type (1 for requirements, 3 for design document, 2 for story breakdown)
- Each sub-agent MUST adopt a distinct persona from the persona tables above, matched to the artifact type being reviewed
- Persona-based reviewers review holistically — do NOT constrain them to a narrow topic checklist. The persona description defines their lens, not their scope.
- Sub-agents MUST produce feedback in the structured format specified above
- You MUST NOT skip the adversarial review for any artifact listed in the persona tables
- In interactive mode: you MUST present all reviewer feedback before asking the human which issues to address
- In interactive mode: you MUST NOT auto-dismiss any reviewer feedback — only the human decides what to address or dismiss
- In auto mode: you MUST NOT exceed 3 review-revision rounds
- In auto mode: in round 2, you MUST only address issues with severity `blocker` or `major`
- In auto mode: after round 3, if any verdict is BLOCK, you MUST emit a rejection event listing all remaining blocker issues
- You MUST revise the artifact in-place after addressing feedback (do not create a new copy)
- If revisions are made, you MUST re-commit the revised artifact (see Commit After Phase section)

**External review revision protocol:**

When an artifact is revised based on external feedback (e.g., PR review comments from the PO, stakeholder feedback, or post-approval rejection), adversarial reviewers MUST re-run on the revised artifact. This protocol covers re-entry into the skill after a human gate returns the work for revision — a fundamentally different flow from crash-recovery resumability.

**Distinguishing revision re-entry from crash recovery:**

Resumability (above) handles interrupted runs — the skill detects existing artifacts and skips completed phases. External revision handles **completed-but-rejected** artifacts that need rework. These are different situations:

| Signal | Resumability (crash recovery) | External revision |
|--------|-------------------------------|-------------------|
| Trigger | Skill interrupted mid-pipeline | Human gate rejects or requests changes |
| Detection | Artifact exists but next phase artifact missing | Artifact exists AND issue returned from a review status (e.g., `human:po:plan-review` → `eng:lead:plan`) or PR has unresolved review comments |
| Action | Skip to next incomplete phase | Re-open the completed phase for targeted revision |

**Constraints (interactive mode):**
- External feedback arrives in-session — the human tells the engineer about PR comments, stakeholder input, or rejection reasons
- You MUST read and summarize the external feedback before revising
- You MUST revise the artifact in-place based on the feedback
- After substantive revisions (changes to content, contracts, or scope — not typos or link fixes), you MUST re-run adversarial reviewers on the **full revised artifact**, not just the diff — revisions may introduce inconsistencies or regressions
- You MUST provide reviewers with a "Changes since last review" summary listing the external feedback items addressed and sections modified
- You MUST present all reviewer feedback to the human before committing — the human decides what to address
- The 3-round limit does not apply in interactive mode — the human decides when the artifact is ready

**Constraints (auto mode):**
- External feedback arrives as a board state change — the issue returns to a planning status after rejection, with review comments on the issue or PR
- At skill re-entry, you MUST check whether the issue was previously in a review status (e.g., `human:po:plan-review`) and returned with rejection feedback. If so, this is an external revision re-entry, NOT a resumability skip.
- You MUST read all rejection comments and PR review feedback before revising
- You MUST revise the artifact in-place, then re-run adversarial reviewers on the full revised artifact
- The 3-round limit resets for each external revision cycle — round counting is separate from the initial review
- After the review-revision cycle completes, you MUST re-submit for human review (re-open PR or transition back to plan-review status)

## Steps

### 1. Project Selection

Determine which BotMinter project (code repository) this epic belongs to.

**Constraints (interactive mode):**
- You MUST scan the `projects/` directory in the workspace to list available projects
- If only one project exists, auto-select it and inform the operator: "Auto-selected project `<name>` (only project in workspace)"
- If multiple projects exist, present the list and ask the operator which project this epic is for
- You MUST confirm the selected project before proceeding
- You MUST store the selected project name for use in constructing `{epic_dir}` = `team/specs/{project}/{issue#}-{epic_name}/`

**Constraints (auto mode):**
- You MUST derive the project from the issue's `project/<name>` label, or from context in the epic body
- If only one project exists in `projects/`, auto-select it
- If the project cannot be determined, you MUST log the ambiguity and halt with an actionable error

### 2. Create Planning Structure

Set up a directory structure to organize all planning artifacts created during the process.

**Constraints:**
- You MUST first check if `{epic_dir}/` already exists with artifacts from a previous run (see Resumability section)
- If existing artifacts are found, you MUST identify the last completed phase and skip to the next incomplete phase:
  - If `plan.md` exists with STORY-NN entries → all phases complete, proceed to Step 10
  - If `design.md` exists with AC-NN entries → resume at Step 9 (plan)
  - If `requirements-manifest.md` exists listing domain files written → resume at Step 8 (design)
  - If `research/` contains R-NN files → resume at Step 7 (requirements)
  - If `idea-honing.md` has Q-NN entries with answers → resume at Step 5 (research)
  - If only `rough-idea.md` exists → resume at Step 3 (process planning)
- In interactive mode: you MUST inform the user which phases are being skipped and why
- In auto mode: you MUST log the resumability detection in the first artifact produced
- You MUST ensure a GitHub Epic issue exists for this work item. If the skill was invoked with an existing issue number (e.g., from a board work item), use that issue. Otherwise, create a new Epic issue using the `github-project` skill with a title derived from the rough idea. The issue body MUST use the stub template:

  ```markdown
  ## Summary

  <1-2 sentence description from the rough idea>

  ## Motivation

  <Why this epic exists — the problem or opportunity>

  ---

  *Specs pending — planning artifacts will be linked here after planning completes.*
  ```

  The issue's initial status MUST be set to `human:po:triage`. The issue number becomes `{issue#}` for the directory name.
- You MUST create the epic directory `team/specs/{project}/{issue#}-{epic_name}/` if it doesn't already exist
- You MUST create the following files:
  - {epic_dir}/rough-idea.md (containing the provided rough idea, with frontmatter field `epic_issue: <number>`)
  - {epic_dir}/idea-honing.md (for requirements clarification)
- You MUST create the following subdirectories:
  - {epic_dir}/research/ (directory for research notes)
- You MUST update `team/specs/index.md` with a new entry for this epic. Create the file if it doesn't exist. Each entry should include the issue number, title, project, and link to the spec directory.
- In interactive mode: you MUST notify the user when the structure has been created and the epic issue has been filed
- You MUST inform the user that all planning artifacts will remain available throughout the process
- You MUST explain that this will ensure all planning artifacts remain in context throughout the process

### 3. Initial Process Planning

Determine the initial approach and sequence for requirements clarification and research.

**Constraints (interactive mode):**
- You MUST ask the user if they prefer to:
  - Start with requirements clarification (default)
  - Start with preliminary research on specific topics
  - Provide additional context or information before proceeding
- You MUST adapt the subsequent process based on the user's preference
- You MUST explain that the process is iterative and the user can move between requirements clarification and research as needed
- You MUST wait for explicit user direction before proceeding to any subsequent step
- You MUST NOT automatically proceed to requirements clarification or research without user confirmation because this could lead the process in a direction the user doesn't want

**Constraints (auto mode):**
- You MUST skip this step entirely — proceed directly to Step 4 (requirements clarification) as the default sequence
- You MUST NOT prompt for user preferences

### 4. Requirements Clarification

Guide the development of a thorough specification through a series of questions. Each question receives a sequential `Q-NN` identifier for cross-referencing.

**Constraints (all modes):**
- You MUST create an empty {epic_dir}/idea-honing.md file if it doesn't already exist
- You MUST assign each question a sequential ID in `Q-NN` format (Q-01, Q-02, ..., Q-14, etc.)
- You MUST format the idea-honing.md document with clear question and answer sections, each prefixed by its Q-NN ID
- You MUST include the final chosen answer in the answer section
- You MAY include alternative options that were considered before the final decision
- You MUST continue asking questions until sufficient detail is gathered
- You SHOULD ask about edge cases, user experience, technical constraints, and success criteria
- You SHOULD adapt follow-up questions based on previous answers
- After completing this phase, you MUST commit `{epic_dir}/idea-honing.md` to version control (see Commit After Phase section)

**Constraints (interactive mode only):**
- You MUST ask ONLY ONE question at a time and wait for the user's response before asking the next question
- You MUST NOT list multiple questions for the user to answer at once because this overwhelms users and leads to incomplete responses
- You MUST NOT pre-populate answers to questions without user input because this assumes user preferences without confirmation
- You MUST NOT write multiple questions and answers to the idea-honing.md file at once because this skips the interactive clarification process
- You MUST follow this exact process for each question:
  1. Formulate a single question and assign it the next `Q-NN` ID
  2. Append the question (prefixed with its Q-NN ID) to {epic_dir}/idea-honing.md
  3. Present the question to the user in the conversation
  4. Wait for the user's complete response, which may require brief back-and-forth dialogue across multiple turns.
  5. Once you have their complete response, append the user's answer (or final decision) to {epic_dir}/idea-honing.md under the corresponding Q-NN entry
  6. Only then proceed to formulating the next question
- You MAY suggest possible answers when asking a question, but MUST wait for the user's actual response
- You MUST ensure you have the user's complete response before recording it and moving to the next question
- You MAY suggest options when the user is unsure about a particular aspect
- You MAY recognize when the requirements clarification process appears to have reached a natural conclusion
- You MUST explicitly ask the user if they feel the requirements clarification is complete before moving to the next step
- You MUST offer the option to conduct research if questions arise that would benefit from additional information
- You MUST be prepared to return to requirements clarification after research if new questions emerge
- You MUST NOT proceed with any other steps until explicitly directed by the user because this could skip important clarification steps

**Constraints (auto mode only):**
- You MUST formulate questions and self-answer them using available context: the epic body, project codebase, team knowledge, and existing artifacts
- You MUST write all questions and self-answers to {epic_dir}/idea-honing.md
- You MUST mark all self-generated answers with `**Answer (agent-derived):**` to clearly distinguish them from human-provided answers
- You MUST document the context source used for each answer (e.g., "Source: epic body", "Source: codebase analysis", "Source: team knowledge")
- You MUST NOT block or wait for human input
- You MAY formulate and answer multiple questions before writing them to the file
- You MUST proceed to the next step automatically when sufficient detail is gathered

**Scope Detection (all modes):**

After completing the initial round of requirements clarification (at least 3-5 questions answered), you MUST evaluate whether the work item is story-scope rather than epic-scope. Story-scope signals:

- **Clear single objective:** The idea describes one well-defined deliverable, not a system or set of features
- **No technology unknowns:** All technologies and patterns are established — no research phase would be needed
- **Single story:** The implementation would be a single story (one PR, one deliverable), not multiple incremental stories
- **No architectural decisions:** No new patterns, no system design trade-offs, no cross-component coordination

If 3 or more story-scope signals are present:

| Mode | Behavior |
|------|----------|
| **Interactive** | Present the assessment to the user: explain which signals were detected, and ask whether to continue with full epic-level PDD or demote to story-level and switch to the story-mgmt skill instead |
| **Auto** | Demote to story-level automatically — stop the PDD pipeline and invoke the story-mgmt skill with the work item as input. Log the scope detection rationale in the work item or artifact before demoting |

**Scope detection constraints:**
- You MUST NOT trigger scope detection before at least 3 questions have been asked and answered — insufficient context leads to false positives
- You MUST list the specific signals detected when proposing a scope change
- In interactive mode: you MUST NOT auto-demote — only the user decides whether to continue with PDD or switch to story-mgmt
- In auto mode: you MUST log the scope detection rationale in the artifact before demoting

**Example idea-honing.md format (interactive mode):**

````markdown
## Q-01: What is the primary use case for this feature?

**Answer:** The primary use case is allowing team leads to create reusable document templates...

## Q-02: Who are the target users?

**Answer:** Internal team members — specifically engineering leads and project managers...

## Q-03: What existing systems does this need to integrate with?

**Answer:** It needs to integrate with the existing auth system (OAuth2) and the document storage API...
````

**Example idea-honing.md format (auto mode):**

````markdown
## Q-01: What is the primary use case for this feature?

**Answer (agent-derived):** Based on the epic body, the primary use case is allowing team leads to create reusable document templates for common deliverables.
Source: epic body

## Q-02: Who are the target users?

**Answer (agent-derived):** The target users are internal team members — specifically engineering leads and project managers, as inferred from the existing user roles in the codebase.
Source: codebase analysis (src/models/user.rs)

## Q-03: What existing systems does this need to integrate with?

**Answer (agent-derived):** It needs to integrate with the existing auth system (OAuth2) and the document storage API, based on the current architecture.
Source: codebase analysis, team knowledge
````

### 5. Research Relevant Information

Conduct research on relevant technologies, libraries, or existing code that could inform the design. Each research topic receives a sequential `R-NN` identifier.

**Constraints (all modes):**
- You MUST identify areas where research is needed based on the requirements
- You MUST document research findings in separate markdown files in the {epic_dir}/research/ directory, with filenames prefixed by the R-NN ID (e.g., `R-01-existing-code.md`, `R-02-technologies.md`)
- You MUST include mermaid diagrams when documenting system architectures, data flows, or component relationships in research
- You MUST include links to relevant references and sources when research is based on external materials (websites, documentation, articles, etc.)
- You MAY use available tools to search code, read files, or fetch web content to gather information
- You MUST summarize key findings that will inform the design
- You SHOULD cite sources and include relevant links in research documents
- After completing this phase, you MUST commit `{epic_dir}/research/` files to version control (see Commit After Phase section)

**Constraints (interactive mode only):**
- You MUST propose an initial research plan to the user, listing topics to investigate with `R-NN` IDs (R-01, R-02, etc.)
- You MUST ask the user for input on the research plan, including:
  - Additional topics that should be researched
  - Specific resources (files, websites, internal tools) the user recommends
  - Areas where the user has existing knowledge to contribute
- You MUST incorporate user suggestions into the research plan
- You MUST ask the user whether other available search tools should also be used
- You MUST periodically check with the user during the research process (these check-ins may involve brief dialogue to clarify feedback) to:
  - Share preliminary findings
  - Ask for feedback and additional guidance
  - Confirm if the research direction remains valuable
- You MUST ask the user if the research is sufficient before proceeding to the next step
- You MUST offer to return to requirements clarification if research uncovers new questions or considerations
- You MUST NOT automatically return to requirements clarification after research without explicit user direction because this could disrupt the user's intended workflow
- You MUST wait for the user to decide the next step after completing research

**Constraints (auto mode only):**
- You MUST determine the research plan autonomously based on the requirements and available context
- You MUST investigate all identified topics without waiting for human guidance
- You MUST document research decisions and topic selection rationale in the research files
- You MUST proceed to the next step automatically when research is sufficient

### 6. Iteration Checkpoint

Determine if further requirements clarification or research is needed before proceeding to design.

**Constraints (interactive mode):**
- You MUST summarize the current state of requirements and research to help the user make an informed decision
- You MUST explicitly ask the user if they want to:
  - Proceed to extracting requirements into a standalone document
  - Return to requirements clarification based on research findings
  - Conduct additional research based on requirements
- You MUST support iterating between requirements clarification and research as many times as needed
- You MUST ensure that both the requirements and research are sufficiently complete before proceeding to the requirements extraction step
- You MUST NOT proceed to the requirements extraction step without explicit user confirmation because this could skip important refinement steps

**Constraints (auto mode):**
- You MUST autonomously assess whether requirements and research are sufficiently complete
- If sufficient: proceed directly to Step 7 (extracting requirements)
- If gaps remain: iterate — return to Step 4 or Step 5 as needed, then re-evaluate
- You MUST document the assessment rationale in the next artifact produced

### 7. Extract Requirements

Extract and formalize requirements from the idea-honing Q&A into the project-level requirements catalogue at `team/specs/{project}/requirements/`.

**Requirement Qualification:**

Requirements describe the **WHAT** — behavior observable by users, operators, or stakeholders.

- **Functional requirements** describe what the system must DO (e.g., "The system MUST allow customers to specify a KMS key ARN for StorageClass encryption")
- **Non-functional requirements** describe quality attributes specific to this feature (e.g., "KMS key validation MUST NOT add more than 500ms to the reconciliation cycle"). Not every feature has NFRs — do not force them.

**Litmus test:** (1) Can a stakeholder verify this requirement without reading source code? If no → design.md. (2) Does the requirement prescribe a specific technology, pattern, algorithm, regex, or internal mechanism? If yes → extract the observable behavior as the requirement, move the mechanism to design.md as a design decision (D-NN).

The following do NOT belong in the requirements catalogue:

| What | Where it belongs | Example |
|------|-----------------|---------|
| Implementation details (type names, struct fields, internal APIs, data propagation) | design.md — Architecture / Components | "MUST set driverType to AWSDriverType", "MUST mirror field from HC to HCP" |
| Technology/pattern choices (specific libraries, algorithms, regex patterns) | design.md — Design Decisions (D-NN) | "MUST use CEL rules, not webhooks", "MUST use regex ^arn:..." |
| Engineering practices (testing, documentation) | design.md — Testing Strategy / Documentation Impact | "MUST include unit tests for validation", "MUST document key rotation" |

**Constraints (all modes):**
- You MUST create or append to domain files in `team/specs/{project}/requirements/`. Create the directory if it doesn't exist.
- You MUST read all Q-NN entries in {epic_dir}/idea-honing.md and extract requirement statements
- You MUST organize requirements by functional domain, where each domain maps to one file
- Domain filenames MUST be 3-5 lowercase characters and serve as the category abbreviation when uppercased (e.g., `kms.md` → `KMS-NN`, `auth.md` → `AUTH-NN`). No full words.
- You MUST assign each requirement a `CATEGORY-NN` ID with zero-padded sequential numbers. When appending to an existing domain file, read it, find the highest existing ID, and continue numbering sequentially.
- Before writing requirements, you MUST list existing files in `team/specs/{project}/requirements/` — appending to an existing domain is the default, creating a new domain is the exception
- In interactive mode: present existing domains and recommend appending unless the feature is genuinely a new functional area
- In auto mode: fuzzy-match domain names and topic overlap — always prefer appending to an existing domain over creating a new one
- A single epic MAY produce requirements across multiple domain files if the feature spans multiple functional areas. Each category maps to exactly one domain file.
- In interactive mode: present the proposed domain grouping (which requirements go to which domain file) for approval before writing
- In auto mode: default to one domain per epic unless requirements clearly span distinct functional areas
- When appending to an existing domain file, you MUST commit the domain file immediately after writing to minimize ID collision risk from concurrent epic planning
- After completing this phase, you MUST commit the domain files in `team/specs/{project}/requirements/` and write `{epic_dir}/requirements-manifest.md` to version control (see Commit After Phase section)
- You MUST use the following document structure for each domain file:

````markdown
# {Domain Name}

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| DOM-01 | [Requirement text using MUST/SHOULD/MAY] | must-have | #54/Q-03, #54/Q-05 |
| DOM-02 | ... | should-have | #54/Q-07 |
````

- You MUST write a requirements manifest at `{epic_dir}/requirements-manifest.md` listing all domain files written and their ID ranges:

````markdown
# Requirements Manifest — {Epic Name}

| Domain File | IDs Added | Count |
|-------------|-----------|-------|
| [kms.md](../requirements/kms.md) | KMS-01 to KMS-07 | 7 |
````

- The **Source** column MUST use `#{issue}/Q-NN` format to identify which epic's idea-honing produced each requirement (e.g., `#54/Q-03`)
- **Priority** MUST be one of: `must-have`, `should-have`, `could-have`, `wont-have`
- Requirement text MUST use RFC 2119 keywords (MUST, SHOULD, MAY) to express obligation level
- You MUST NOT create categories for testing or documentation — these are engineering practices, not requirements. Testing strategy and documentation needs are captured in the design document's dedicated sections (Testing Strategy, Documentation Impact)
- You MUST NOT include implementation details as requirements — if a requirement prescribes a specific technology, names an internal API, or describes internal data propagation, extract only the observable behavior as the requirement and move the mechanism to the design document
- After extraction, you MUST sanity-check the count: if a feature produces more than 15 requirements, re-evaluate each against the litmus test — a high count is a smell for implementation details leaking into requirements

**Adversarial Review (requirements):**
- After producing and committing the requirements domain files, you MUST run the adversarial review process (see Adversarial Review section)
- Use the **Requirements** persona: Product Manager
- Apply the iteration protocol matching the current mode (interactive or auto)
- When appending to an existing domain file, the reviewer MUST review only the newly added entries from this epic (identified by Source column `#{issue}/Q-NN`). The reviewer SHOULD flag conflicts with existing entries but MUST NOT re-litigate requirements approved in a previous epic's review cycle.
- If revisions are made, re-commit the domain files in `team/specs/{project}/requirements/` with message `docs(specs): revise requirements after review for {epic_name}`

**Constraints (interactive mode only):**
- You MUST present the extracted requirements to the user for review
- You MUST iterate on the requirements based on user feedback
- You MUST NOT proceed to the design step without explicit user confirmation
- You MUST offer to return to requirements clarification if gaps are identified during extraction

**Constraints (auto mode only):**
- You MUST extract and categorize all requirements autonomously
- You MUST proceed directly to Step 8 after requirements extraction
- You MUST NOT block or wait for human review

### 8. Create Detailed Design

Develop a comprehensive design document based on the requirements and research. The design document references requirements by their `CATEGORY-NN` IDs from the project requirements catalogue rather than duplicating requirement text.

**Constraints (all modes):**
- You MUST create a detailed design document at {epic_dir}/design.md
- After completing this phase, you MUST commit `{epic_dir}/design.md` to version control (see Commit After Phase section)
- You MUST write the design as a standalone document that can be understood without reading other planning artifacts
- You MUST include the following sections in the design document:
  - Overview
  - Requirements Summary (reference the project requirements catalogue by CATEGORY-NN IDs with relative markdown links to domain files — do NOT duplicate full requirement text)
  - Architecture Overview
  - Components and Interfaces
  - Data Models
  - Error Handling
  - Acceptance Criteria (each tagged with `AC-NN` ID — AC-01, AC-02, etc.)
  - Design Decisions (each tagged with `D-NN` ID — D-01, D-02, etc.)
  - Testing Strategy
  - Security Considerations (threat model, auth boundaries, input validation, data exposure risks)
  - Observability (logging, monitoring, metrics, alerting, debugging approach)
  - Performance (latency, throughput, resource usage, scalability considerations)
  - Migration & Compatibility (backward compatibility, data migration, rollout strategy)
  - Impact on Existing System (what changes, what breaks, blast radius, changed signatures)
  - Documentation Impact (user-facing docs, API docs, changelog entries needed)
  - Appendices (Technology Choices, Research Findings, Alternative Approaches)
  - Traceability Matrix (LAST section — maps requirements to acceptance criteria and stories)
- **Abstraction Level:** The design document describes WHAT the system does at component boundaries — contracts, interfaces, data flow, and behavioral guarantees. It MUST NOT prescribe HOW components implement their responsibilities internally. The litmus test: "If a story implemented this design using different function names, file organization, or internal patterns, would this section need to change?" If yes, the section is too implementation-specific.
  - **DO** describe: component responsibilities, API contracts (field names, types, validation rules, condition types/reasons), data flow between components, behavioral guarantees, error handling contracts
  - **DO NOT** prescribe: function names, file paths, step-by-step implementation instructions, internal struct field assignments, specific code patterns. These are discovered during implementation and belong in story subtasks.
  - **Exception:** Exact field names and types for API surfaces (e.g., CRD fields, CLI flags, condition types) ARE design-level because they are external contracts. Internal function signatures and file paths are NOT.
  - When referencing project-specific conventions or context files (e.g., `api/CLAUDE.md`), point stories to those files rather than inlining the implementation rules into the design.
- Acceptance criteria operationalize requirements into concrete, testable scenarios. Each AC specifies a precondition, action, and expected observable outcome that verifies one or more requirements are satisfied. The requirement (CATEGORY-NN) states the obligation (WHAT); the AC (Given-When-Then) makes it verifiable.
- You MUST assign each acceptance criterion a sequential `AC-NN` ID (AC-01, AC-02, ...). Acceptance criteria MUST be in Given-When-Then (GWT) format and reference the `CATEGORY-NN` requirement(s) they verify.
- You MUST assign each design decision a sequential `D-NN` ID (D-01, D-02, ...). Each decision MUST include the chosen option, alternatives considered, and rationale.
- The Requirements Summary section MUST reference the project requirements catalogue by CATEGORY-NN IDs using relative markdown links to the domain files (e.g., `[kms.md](../requirements/kms.md)`) and MUST NOT duplicate the full requirement text. A brief paraphrase or the requirement title is acceptable for readability, but the authoritative text lives in `team/specs/{project}/requirements/{domain}.md`. In actual artifacts, reference these files using relative markdown links.
- All file cross-references in planning artifacts (design.md, plan.md, requirements-manifest.md) MUST use relative markdown links, not plain-text paths. Relative paths assume artifacts are at `team/specs/{project}/{epic_dir}/` and requirements at `team/specs/{project}/requirements/`, so the relative path is `../requirements/{domain}.md`. Template patterns with `{project}` or `{domain}` placeholders in this skill document are instructional — replace with concrete relative links in actual artifacts.
- You MUST include an appendix section that summarizes key research findings, including:
  - Major technology choices with pros and cons
  - Existing solutions analysis
  - Alternative approaches considered
  - Key constraints and limitations identified during research
- You SHOULD include diagrams or visual representations when appropriate using mermaid syntax
- You MUST generate mermaid diagrams for architectural overviews, data flow, and component relationships
- You MUST ensure the design addresses all requirements identified in the project requirements catalogue for this epic
- You SHOULD highlight design decisions and their rationales, referencing research findings where applicable
- You MUST include a Traceability Matrix as the LAST section of the design document (after Appendices)
- The Traceability Matrix MUST list every requirement (`CATEGORY-NN`) from the project requirements catalogue relevant to this epic, mapped to the acceptance criteria (`AC-NN`) defined in this design document that verify it
- The **Story** column MUST initially contain `—` (populated in Step 9 after the plan is produced)
- The **Verification Status** column MUST initially contain `Pending`
- Every requirement ID from the project requirements catalogue relevant to this epic MUST appear in the matrix — a missing requirement indicates a gap in the design that must be addressed before proceeding

**Example traceability matrix format (in design.md):**

````markdown
## Traceability Matrix

| Requirement | Acceptance Criteria | Story | Verification Status |
|-------------|--------------------|--------------------|---------------------|
| AUTH-01 | AC-01, AC-02 | — | Pending |
| AUTH-02 | AC-03 | — | Pending |
| TMPL-01 | AC-04, AC-05 | — | Pending |
````

**Constraints (interactive mode only):**
- You MUST review the design with the user and iterate based on feedback
- You MUST explicitly ask the user if they are ready to proceed to implementation before moving to Step 9
- You MUST NOT proceed to the story breakdown step without explicit user confirmation because this could skip important design refinement
- You MUST offer to return to requirements clarification or research if gaps are identified during design

**Constraints (auto mode only):**
- You MUST produce the complete design document autonomously
- You MUST document all design decisions and their rationale inline in the document
- You MUST NOT block or wait for human review — the human reviews the complete output at the plan-review gate

**Adversarial Review (design document):**
- After producing and committing the design document, you MUST run the adversarial review process (see Adversarial Review section)
- Use the **Design Document** personas: Staff Engineer, UX Engineer, QE Engineer
- Apply the iteration protocol matching the current mode (interactive or auto)
- If revisions are made, re-commit `{epic_dir}/design.md` with message `docs(specs): revise design after review for {epic_name}`
- In interactive mode: complete the adversarial review before asking the user to proceed to Step 9
- In auto mode: complete the adversarial review (up to 3 rounds) before proceeding to Step 9 — if blockers remain after 3 rounds, emit a rejection event

**Example acceptance criterion format:**

````markdown
**AC-01:** Given [precondition referencing CATG-NN], when [action], then [observable outcome].
````

**Example decision format:**

````markdown
**D-01:** [Decision title]
- **Chosen:** [option]
- **Alternatives:** [other options considered]
- **Rationale:** [why this option was chosen, referencing R-NN research if applicable]
- **ADR:** ADR-NNNN (generated by ADR skill)
````

**ADR generation:**
- When an architectural decision (`D-NN`) is made during design, you MUST invoke the ADR skill to generate a formal `ADR-NNNN` document
- The `D-NN` entry in the design document is the lightweight inline record; the ADR skill produces the full formal document with context, decision rationale, alternatives, and consequences
- Pass the decision title and context (including the `D-NN` ID and the design document path) to the ADR skill
- After the ADR skill returns the `ADR-NNNN` ID, you MUST add an `**ADR:** ADR-NNNN` line to the corresponding `D-NN` entry in the design document
- ADRs are generated in `team/specs/adrs/` (the ADR skill's default directory)
- In interactive mode: the ADR skill presents the proposed ADR to the user for review before writing
- In auto mode: the ADR skill generates the ADR autonomously — all ADRs are committed as part of the design document commit-after-phase

### 9. Develop Story Breakdown

Create a structured story breakdown with a series of stories for implementing the design. Each story receives a sequential `STORY-NN` identifier.

**Constraints (all modes):**
- You MUST create a story breakdown at {epic_dir}/plan.md
- After completing this phase, you MUST commit `{epic_dir}/plan.md` to version control (see Commit After Phase section)
- You MUST include a checklist at the beginning of the plan.md file to track story progress
- You MUST use the following specific instructions when creating the story breakdown:
  ```
  Convert the design into a series of stories that will build each component in a test-driven manner following agile best practices. Each story must result in a working, demoable increment of functionality. Prioritize best practices, incremental progress, and early testing, ensuring no big jumps in complexity at any stage. Make sure that each story builds on the previous stories, and ends with wiring things together. There should be no hanging or orphaned code that isn't integrated into a previous story.
  ```
- You MUST assign each story a sequential `STORY-NN` ID (STORY-01, STORY-02, etc.)
- You MUST format the story breakdown as a numbered series of detailed stories, each prefixed with its STORY-NN ID
- Each story MUST be written as a clear implementation objective
- Each story MUST begin with "STORY-NN:" where NN is the zero-padded sequential number
- The format MUST be directly usable by the breakdown hat to create a GitHub story issue without transformation. Each STORY-NN maps 1:1 to a story issue.
- You MUST ensure each story includes:
  - **Title** — a concise, issue-title-ready summary (e.g., "Add OAuth2 token refresh endpoint")
  - **Objective** — what this story delivers as a working increment
  - **Implementation Guidance** — general approach, not excessive detail (the design doc has the details)
  - **Test Requirements** — what tests are written as part of this story
  - **Integration** — how this story connects to previous and subsequent stories
  - **Demo** — explicit description of the working functionality that can be demonstrated after completing this story
  - **Requirements** — which `CATEGORY-NN` requirement(s) this story addresses
  - **Acceptance Criteria** — which `AC-NN` criterion/criteria this story satisfies
  - **Dependencies** — which `STORY-NN` stories must be complete before this one can start (use `—` if none)
- You MUST ensure each story results in working, demoable functionality that provides value
- You MUST sequence stories so that core end-to-end functionality is available as early as possible
- You MUST NOT include excessive implementation details that are already covered in the design document because this creates redundancy and potential inconsistencies
- You MUST assume that all context documents (requirements, design, research) will be available during implementation
- You MUST break down the implementation into a series of discrete, manageable stories
- You MUST ensure each story builds incrementally on previous stories
- You MUST structure each story so that tests are written before or alongside the implementation code
- You MUST include test requirements as part of each story that introduces or modifies functionality, not as separate testing-only stories
- You MUST NOT create stories that are solely dedicated to testing or "adding tests" for functionality implemented in previous stories because this violates test-driven development principles and allows untested code to accumulate
- You MUST ensure the plan covers all aspects of the design
- You SHOULD sequence stories to validate core functionality early
- You MUST ensure the checklist items correspond directly to the stories in the plan, using the STORY-NN IDs
- After the plan is finalized, you MUST update the Traceability Matrix in design.md: fill in the **Story** column with the `STORY-NN` IDs from the plan for each requirement

**Constraints (auto mode only):**
- You MUST produce the complete story breakdown autonomously without user confirmation

**Adversarial Review (story breakdown):**
- After producing and committing the story breakdown, you MUST run the adversarial review process (see Adversarial Review section)
- Use the **Story Breakdown** personas: Staff Engineer, Delivery/PM
- Apply the iteration protocol matching the current mode (interactive or auto)
- If revisions are made, re-commit `{epic_dir}/plan.md` with message `docs(specs): revise plan after review for {epic_name}`
- In interactive mode: complete the adversarial review before proceeding to Step 10
- In auto mode: complete the adversarial review (up to 3 rounds) before proceeding to Step 10 — if blockers remain after 3 rounds, emit a rejection event

### 10. Summarize and Present Results

Provide a summary of all artifacts created and next steps.

**Constraints (all modes):**
- You MUST create a summary document at {epic_dir}/summary.md
- You MUST list all artifacts created during the process with their ID counts:
  - Number of questions (Q-NN) in idea-honing.md
  - Number of requirements (CATEGORY-NN) in the project requirements catalogue, broken down by domain
  - Number of research topics (R-NN)
  - Number of acceptance criteria (AC-NN) in design.md
  - Number of decisions (D-NN) in design.md
  - Number of stories (STORY-NN) in plan.md (story breakdown)
- You MUST provide a brief overview of the design and story breakdown
- You MUST suggest next steps for the user
- You SHOULD highlight any areas that may need further refinement

**Constraints (interactive mode only):**
- You MUST present this summary to the user in the conversation

**Constraints (auto mode only):**
- You MUST write the summary to `{epic_dir}/summary.md` without presenting it conversationally
- You MUST provide a brief completion notice documenting the total artifact counts and any areas flagged for human review

**PR and Status Transition (interactive mode only):**

You MUST:

1. **Open a PR** on the team repo with the spec artifacts, linked to the epic issue. The PR title MUST reference the epic (e.g., `[#1] Planning: Tmux agent sessions for observability`). The PR body MUST summarize the artifacts produced and MUST include `Resolves #<epic-issue-number>` to link the PR to the epic in the Development sidebar.
2. **Add the PR to the project board** with status `human:po:plan-review`. The PR MUST be added as a project item so it is visible in the board-driven workflow and the PO can find it alongside the epic.
3. **Move the epic issue** to `human:po:plan-review` using the `status-workflow` skill.
4. **Inform the user** that the PR is open, on the board, and the epic is in plan-review.

**Skill Chaining (interactive mode only):**

After the PR is opened, you MUST offer to continue with story creation:

- Ask the user: "Would you like to proceed with creating story issues and decomposing tasks now, or stop here and review the PR first?"
- If the user wants to **stop here**: end the skill. The user will review and merge the PR externally. The board scanner picks up the approved epic at `eng:lead:breakdown`.
- If the user wants to **continue in this session**:
  1. The user reviews the plan during the conversation. When they approve:
  2. Check if the spec PR is still open. If so, confirm with the user: "The spec PR needs to be merged before breakdown. Should I merge it now?"
  3. If confirmed, merge the PR using the `github-project` skill.
  4. **Enrich the epic issue body** using the Epic Issue Body Enrichment template (see below). This MUST happen after the PR is merged so that all `main` branch links resolve.
  5. Move the epic to `eng:lead:breakdown`.
  6. Run the **Breakdown Operation** (see below) to create story issues, populate bodies, and update the epic body.
  7. Load the `story-mgmt` skill and chain into it for each story for task decomposition.
  8. For each story, the story-mgmt follows the same pattern: produce task files → open a PR → move the story to `human:po:plan-review`.
  9. You MUST pass the relevant planning context (design.md path, requirements catalogue path `team/specs/{project}/requirements/`, `CATEGORY-NN` and `AC-NN` IDs) when chaining into story-mgmt.
- If the user chooses to create stories AND decompose tasks, you MUST ask about sequencing:
  - **All stories first:** Create all story issues from the plan, then decompose each story into tasks
  - **Story-by-story:** Create one story issue, decompose it into tasks, then move to the next story

**PR and Status Transition (auto mode only):**

- You MUST NOT open a PR — the specs are committed directly to the team repo
- You MUST update the epic issue body with the enriched template (see "Epic Issue Body Enrichment" below) — since specs are committed directly to `main`, links resolve immediately
- You MUST NOT transition the epic status — the calling hat's quality gate handles the transition to `human:po:plan-review` after adversarial review passes
- You MUST NOT post comments — the calling hat handles comment attribution
- You MUST NOT chain into story-mgmt or create story issues — that happens via the Breakdown Operation after plan approval

**Epic Issue Body Enrichment (both modes):**

This operation updates the epic issue body with the enriched template, replacing the stub body created in Step 2. In interactive mode, this MUST run after the spec PR is merged so that all `main` branch links resolve. In auto mode, specs are committed directly to `main`, so links resolve immediately.

1. Read the spec artifacts: `summary.md`, `design.md`, `plan.md`, and domain files from `team/specs/{project}/requirements/` (use the requirements manifest to identify relevant domain files)
2. Derive `TEAM_REPO` from `.botminter.workspace` field `team_repo` (workspace root) or from `git -C team remote get-url origin`
3. Construct the enriched body using this template:

   ```markdown
   ## Summary

   <2-3 sentences from summary.md>

   ## Motivation

   <Why this epic exists — from idea-honing or summary>

   ## Key Design Decisions

   - **D-01**: <title> — <chosen option>
   - **D-02**: <title> — <chosen option>

   ## Requirements

   | ID | Requirement | Stories |
   |----|-------------|---------|
   | [CATG-01](https://github.com/<TEAM_REPO>/blob/main/specs/<project>/requirements/<domain>.md) | <full requirement text> | |
   | [CATG-02](https://github.com/<TEAM_REPO>/blob/main/specs/<project>/requirements/<domain>.md) | <full requirement text> | |

   ## Specs

   - [Requirements](https://github.com/<TEAM_REPO>/tree/main/specs/<project>/requirements/) — domains: {list of domain files touched}
   - [Design](https://github.com/<TEAM_REPO>/blob/main/specs/<project>/<issue#>-<slug>/design.md) — N ACs, N decisions
   - [Plan](https://github.com/<TEAM_REPO>/blob/main/specs/<project>/<issue#>-<slug>/plan.md) — N stories
   - [Research](https://github.com/<TEAM_REPO>/tree/main/specs/<project>/<issue#>-<slug>/research/)
   ```

   Notes:
   - The Stories column in the Requirements table is **left empty** at this point — story issue numbers don't exist yet. They are filled in after story creation (phase 2, see below).
   - If any artifact file cannot be read, skip that section and add `*[Section unavailable — artifact not found]*`.
   - Design decisions are one-liners — the design doc has the full rationale.
   - ACs are NOT listed in the epic body — they are distributed across stories, each story has its own full ACs.

4. Write the body to a temp file and call `update-issue.sh --issue <N> --body-file <path>` to update the epic issue body.

## Artifact Summary

The complete PDD pipeline produces the following artifacts:

| Artifact | Location | IDs | Content |
|----------|----------|-----|---------|
| `rough-idea.md` | {epic_dir}/ | — | The original idea verbatim |
| `idea-honing.md` | {epic_dir}/ | Q-NN | Question-and-answer pairs from requirements clarification |
| `requirements/{domain}.md` | team/specs/{project}/requirements/ | CATEGORY-NN | Project-level requirements catalogue, organized by functional domain |
| `requirements-manifest.md` | {epic_dir}/ | — | Lists domain files written and ID ranges for this epic |
| `research/*.md` | {epic_dir}/research/ | R-NN | Research notes organized by topic |
| `design.md` | {epic_dir}/ | AC-NN, D-NN | Design document referencing requirements by CATEGORY-NN, with traceability matrix |
| `plan.md` | {epic_dir}/ | STORY-NN | Story breakdown, each story referencing CATEGORY-NN and AC-NN |
| `summary.md` | {epic_dir}/ | — | Summary listing all artifacts and next steps |

## Breakdown Operation

This operation creates story issues from an approved epic plan. It can be invoked in two ways:

1. **As part of interactive skill chaining** (Step 10, when the user continues in-session)
2. **Standalone via the breakdown entry point** (auto mode, invoked by the `lead_breakdown` hat)

**Parameters for breakdown entry point:**
- `issue` (required): Epic issue number
- `project` (required): Project name (derived from `project/<name>` label)

**Behavior:**

1. You MUST read the epic issue to get labels and locate the spec directory at `team/specs/<project>/<issue#>-<slug>/`.
2. **Epic body enrichment check:** You MUST read the current epic issue body. If it still contains the stub marker (`*Specs pending*`) or lacks the enriched template sections (Requirements table, Specs links), you MUST run the Epic Issue Body Enrichment operation (see above) before proceeding. You MUST NOT create story issues against an un-enriched epic body.
3. You MUST read `plan.md` for the STORY-NN list with their titles, objectives, requirements, ACs, guidance, demos, and dependencies.
4. You MUST read domain files from `team/specs/{project}/requirements/` for full requirement text and `design.md` for full AC GWT text.
5. You MUST derive `TEAM_REPO` from `.botminter.workspace` field `team_repo` or from `git -C team remote get-url origin`.
6. You MUST create each story as a Story-type sub-issue of the parent Epic using the `github-project` skill's sub-issue operation. Each story body MUST follow the **Story Body Template** defined in the `story-mgmt` skill's "Story Issue Body Convention" section. You MUST populate it by reading:
   - `plan.md` for objective, implementation guidance, demo, dependencies
   - the domain file matching the category prefix for full requirement text (e.g., KMS-01 → `team/specs/{project}/requirements/kms.md`)
   - `design.md` for full AC GWT text (look up each AC-NN ID)
   - Spec URLs MUST use `https://github.com/<TEAM_REPO>/blob/main/specs/<project>/<issue#>-<slug>/`
   - Dependencies MUST use `#N` issue links when the story's issue number is known (created earlier); otherwise STORY-NN IDs MAY be used
   - You SHOULD write body to a temp file and use `--body-file` when large
7. **Label propagation:** If the parent epic has the `plan:auto` label, you MUST apply `plan:auto` to each created story using `update-issue.sh --issue <N> --add-label plan:auto`.
8. You MUST set each story's initial project status:
   - Regular stories → `eng:lead:plan`
   - Documentation stories (with `kind/docs` label) → `eng:cw:write`
9. **Phase-2 epic body update:** You MUST update the epic issue body's Requirements table to add `#N` story issue links in the Stories column. You MUST read the current epic body, populate the Stories column with the story issue numbers that cover each requirement (from plan.md's per-story requirement listings), write to a temp file, and call `update-issue.sh --issue <epic#> --body-file <path>`.
10. **Verify:** You MUST confirm each created story is a sub-issue of the parent epic, has the correct initial status set, and has a body containing full requirement text and AC GWT text (not bare IDs). If any verification fails, you MUST raise an error — you MUST NOT return partial results.

**Return value:** The list of created story issue numbers, their STORY-NN mappings, and verification status.

**What this operation does NOT do (caller responsibilities):**
- You MUST NOT transition the epic to `eng:lead:monitor` — the calling hat or chaining flow handles that
- You MUST NOT post comments — the calling hat handles comment attribution

## Examples

### Example Input

```
Rough idea: I want to build a feature for our team's internal tool that allows users to create and manage templates for common documents we produce. Users should be able to create, edit, and share templates, as well as use them to generate new documents with custom fields.

Project: my-project
Epic name: template-feature
```

### Example Output

```
# Research Phase Interaction

Based on your requirements, I've identified several areas that would benefit from research:

1. R-01: **Existing template solutions** — Understanding what's already available
2. R-02: **Storage and versioning approaches** — How to handle template versions and sharing
3. R-03: **Custom field validation patterns** — Best practices for dynamic field validation

I notice you have several additional search tools available. Should I incorporate these additional search tools into the research process for broader coverage of template management solutions and industry best practices?

---

# Planning Summary

I've completed the transformation of your rough idea into a detailed design with a story breakdown. Here's what was created:

## Directory Structure
- team/specs/my-project/15-template-feature/
  - rough-idea.md (your initial concept)
  - idea-honing.md (Q-01 through Q-12 — our requirements clarification)
  - requirements-manifest.md (domain files written for this epic)
  - research/
    - R-01-existing-templates.md
    - R-02-storage-options.md
    - R-03-external-solutions.md
  - design.md (AC-01 through AC-08, D-01 through D-03)
  - plan.md (STORY-01 through STORY-12, story breakdown with checklist)
  - summary.md (this document)
- team/specs/my-project/requirements/
  - auth.md (AUTH-01 through AUTH-03)
  - tmpl.md (TMPL-01 through TMPL-05)
  - share.md (SHARE-01 through SHARE-02)

## Key Design Elements
- Template management system with CRUD operations
- Role-based access control for sharing
- Versioning system for templates
- Custom fields with validation
- Document generation engine

## Implementation Approach
The story breakdown contains 12 stories (STORY-01 through STORY-12), starting with core data models and building up to the complete feature set. Each story references the requirements (CATEGORY-NN) and acceptance criteria (AC-NN) it addresses.

## Next Steps
1. Review the detailed design document at team/specs/my-project/15-template-feature/design.md
2. Check the story breakdown and checklist at team/specs/my-project/15-template-feature/plan.md
3. Begin implementation following the checklist in the story breakdown

Would you like me to explain any specific part of the design or story breakdown in more detail?
```

## Troubleshooting

### Requirements Clarification Stalls
If the requirements clarification process seems to be going in circles or not making progress:
- You SHOULD suggest moving to a different aspect of the requirements
- You MAY provide examples or options to help the user make decisions
- You SHOULD summarize what has been established so far and identify specific gaps
- You MAY suggest conducting research to inform requirements decisions

### Research Limitations
If you cannot access needed information:
- You SHOULD document what information is missing
- You SHOULD suggest alternative approaches based on available information
- You MAY ask the user to provide additional context or documentation
- You SHOULD continue with available information rather than blocking progress

### Design Complexity
If the design becomes too complex or unwieldy:
- You SHOULD suggest breaking it down into smaller, more manageable components
- You SHOULD focus on core functionality first
- You MAY suggest a phased approach to implementation
- You SHOULD return to requirements clarification to prioritize features if needed
