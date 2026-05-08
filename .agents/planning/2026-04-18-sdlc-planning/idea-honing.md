# Requirements Clarification

## Additional Context (provided before Q&A)

**Operator has used and valued aspects of:**
- Agent SOP's PDD skill (planning methodology — what we just installed)
- Agent SOP's code-task-generator and code-assist SOPs (implementation pipeline)
- "Getting Shit Done" (GSD) framework — includes acceptance and user acceptance processes

**Current state:** Planning exists as a workflow step in the SDLC but was under-designed. It's a status an epic passes through, not a defined process with outputs.

**Desired state:**
- Predefined planning AND acceptance process built into the team profile
- These processes generate artifacts
- Skills that can be triggered from multiple entry points: GitHub issue transitions, CLI chat, bridge chat
- Cherry-pick the best parts from Agent SOP PDD and GSD
- Consider incorporating GSD's acceptance/user acceptance into the process

**Key design question:** What to take from PDD vs GSD, and whether acceptance (including user acceptance from GSD) gets wired into the same redesign.

---

## Q&A

### Q1: Is the scope just planning, or does it also include acceptance/verification?

**A1:** Both. The redesign covers the planning phase AND the acceptance/verification side.

### Q2: Where should acceptance/verification live in the lifecycle?

**A2:** This opened up a broader set of concerns:

**Friction point — no direct "implement this" track.** Sometimes the operator already has the plan and just wants the agent to take it and implement directly. The current process forces everything through the full epic lifecycle. We need to rethink granularity and revisit some steps.

**Two mandatory human touch points for any implemented work:**

1. **Specs** (before implementation) — the operator hands over expectations. Could be requirements only, or requirements + ADR + design — whatever artifacts we define. This is the "here's what I want" handoff.

2. **Verification** (after implementation) — the operator becomes aware of what was implemented. This is the "here's what you got" handoff.

**Verification is not always gated.** Two modes exist conceptually:
- **Gated (acceptance):** Human must approve before work is considered done. Blocks progress.
- **Post-hoc (awareness):** Work proceeds, but the human still needs to become aware of what happened. Not blocking, but still required.

**Open questions (not to be finalized now):**
- What constitutes a gate? PR merge? Story closure? Something else?
- If stories close automatically, where is it tracked that the human still has things to catch up on? Maybe at the epic level? TBD.
- The mechanism for gating vs. awareness is a design decision we'll work out later — the principle is established (two touch points required), the implementation details are not.

**Key insight:** Gating and awareness are separate concerns. Auto-merge by sentinel doesn't obsolete the need for human awareness — it just means awareness must happen through a different mechanism.

**Awareness is not passive.** It's not just "know what happened" — it's assessing what was implemented and identifying gaps. This is what GSD's verify-work does: extracts testable deliverables, walks through them, and on failures triggers diagnosis + fix plans. The awareness touch point is an active evaluation, not a notification.

### Q3: What granularity of work should the SDLC support for direct handoff?

**A3:** All three (task, plan/story, epic) — but they are NOT completely distinct workflows. They are **incremental and complementary levels** within one system.

**The hierarchy (largest to smallest):**
- Roadmap → Epics → Stories (sub-stories) → Tasks (sub-tasks)

**Key principle — entry at any level:**
The user is free to start at any level without requiring the parent:
- Start a task without a story
- Start a story without an epic
- Start an epic without a roadmap

**The entry point determines the sub-workflow:**
Based on where you enter, the system knows what planning depth applies. Starting with an epic triggers the full planning pipeline (requirements → research → design → breakdown). Starting with a task triggers the lightweight spec + implement + verify cycle.

**All levels share the same two-touch-point contract** (specs in, verification out), but the weight scales with the level.

**Open question:** What exactly are the levels? Need to define them before defining the sub-workflows.

### Q4: What are the work item levels?

**Note:** GitHub supports custom issue types — if the current profile uses the wrong GitHub type for something (e.g., Task instead of Story), that's a bug to fix as part of this plan. Focus on desired state, not current GitHub constraints.

**A4:** Candidate hierarchy:
- **Roadmap**
- **Milestone**
- **Sprint**
- **Epic**
- **Story**
- **Task**
- **Bug**
- **Spike** (?)

**Resolved — two categories:**

**Structural concepts (containers/time-boxes, not issue types):**
- **Roadmap** — a document/view that groups milestones, not an issue with a lifecycle
- **Milestone** — maps to GitHub native milestones, groups work items
- **Sprint** — a time-box that pulls work in, not an issue type

**Issue types (have lifecycles, statuses, assignees):**
- **Epic** — large body of work, decomposes into stories
- **Story** — single deliverable unit of work
- **Task** — lightweight atomic unit, the "just do this" level
- **Bug** — its own track (currently has simple + complex paths)
- **Spike** — research/investigation task, "I need to learn before I can plan/design"

Open questions:
- What's the boundary between Story and Task? (see Q6)
- Where does Bug sit in the hierarchy — parallel to Story? Its own track at multiple levels?
- Spike: does it produce artifacts that feed into planning (e.g., a research doc that informs a design)?

### Q6: What's the boundary between Story and Task? + Task visibility

**A6 (partial — Story vs Task boundary):** *(pending, superseded by deeper discussion about task nature)*

**A6 (task visibility and agent-created tasks):**

There are two kinds of tasks in the system:

1. **Human-facing tasks** — tasks the human creates or expects as part of the SDLC (epics, stories, etc.). These are "expected artifacts" — the human looks at them, gates on them, reviews them.

2. **Coding-agent-internal tasks** — the task breakdown the coding agent creates for itself during implementation (like Claude Code's internal to-do list). These are implementation-level decomposition that the agent needs to track its own work.

**The value of externalizing agent-internal tasks to GitHub:**
- Makes the agent's work visible on a durable shared medium
- Lets the human see progress (like watching Claude Code's to-do list complete)
- Useful for understanding HOW something was implemented after the fact

**The problem:** These create noise on the board. Sub-tasks moving to done is clutter in the default view, even though the information is sometimes useful.

**Desired behavior — configurable at multiple levels:**

1. **Profile level (team creation time):** Option to say "coding agent tasks are always on" or "always off" for this team. A default policy.

2. **Work item level (per epic/story):** Option to say "for this one, yes, show me the agent's internal tasks" or "for this one, no." Can be dictated upfront or asked.

3. **Board view level:** Default board view does NOT show agent-internal tasks. A separate view (or filter) surfaces them when needed.

4. **Labeling:** Agent-internal tasks MUST be labeled distinctly (e.g., `agent-internal` or similar) so they are:
   - Clearly distinguishable from human-facing work items
   - Filterable in board views
   - Identifiable as agent-created implementation-level decomposition

**Key distinction:** Epics, stories, etc. may ALSO be created by the agent — but those are "expected artifacts" that the human looks for and reviews. The label is not "created by agent" but specifically "agent-internal implementation task" — a different semantic.

### Q8: Sizing recalibration and two levels of planning

**Context:** In the original BotMinter design, issue sizes were intentionally downsized one level vs. typical human teams to make room for agent sub-tasks. What a human team would call a "story" was called an "epic" in BotMinter, and the agent's implementation breakdown filled the story level.

**A8:** With the separation of Tasks (agent-internal) from Stories (human-facing), the sizing can be corrected:
- What was an "Epic" before is now correctly a **Story** (a single deliverable unit)
- **Epic** can now mean what it normally means — a large body of work spanning multiple stories
- **Task** is the atomic level (and agent-internal tasks are a configurable subset of this)

**Two levels of planning, each with two human touch points:**

| Level | Specs touch point (in) | Planning depth | Verification touch point (out) |
|-------|----------------------|----------------|-------------------------------|
| **Epic** | Full planning: requirements, research, design, story breakdown | Heavy (PDD-style) | ? |
| **Story** | Acceptance criteria + light design if needed | Medium | ? |

**A8:** Decouple artifact creation from when they become mandatory:

- **Story cannot start implementation without planning artifacts.** This is the gate.
- **Where artifacts come from** depends on context:
  - Standalone story → artifacts live on the story itself
  - Story from epic → artifacts were produced at epic level (inherited)
- **Planning artifacts are ALWAYS produced at full depth** — requirements, ADRs, designs — regardless of how the work started. Every change that goes in ends up with proper artifacts. The size of artifacts is the same.
- **What varies is WHO produced them and whether the human was involved:**
  - **Collaborative:** Human and AI produce artifacts together in a chat session (PDD-style). Human is deeply involved in shaping them.
  - **Autonomous:** Human provides rough context (epic body, rough idea) and the AI generates all planning artifacts autonomously, with the assumption it has the human's blessing to proceed into implementation.
  - **Human-authored (edge case):** Human writes artifacts themselves and attaches them. The AI works from those.
- **The difference is not artifact depth — it's the level of human involvement in producing them.**
- **The autonomous member must be able to determine the planning mode.** When an agent (e.g., engineer-bob running in Ralph) picks up an epic from the board, it needs to decide:
  1. **Full autonomous** — "I have the human's blessing to generate all artifacts and proceed to implementation." Agent generates requirements, designs, ADRs, breaks down into stories, and implements.
  2. **Produce-and-wait** — "I should generate the planning artifacts non-conversationally and present them to the human for review." Agent does the work, but stops and gates on human approval before implementation.
  3. **Wait-for-artifacts** — "The planning artifacts are expected to come from the human (via a separate chat session, bridge, or manual attachment). I should not generate them myself." Agent parks the epic and waits for the artifacts to appear. The human will produce them in a separate conversation (terminal, Matrix, etc.) — NOT in the same Ralph loop session. The agent simply checks: are the artifacts there yet?

**This decision must be derivable** from signals on the work item — labels, fields, the nature of what's provided (rough context only vs. detailed requirements vs. explicit "go autonomous" flag), or profile-level defaults. The agent should not have to guess.

**The planning UX problem:**

The current GitHub-issue-based back-and-forth (architect designs → lead reviews → human approves/rejects) works for the happy path but is clunky in practice because:
1. Current LLM quality requires significant upfront time defining requirements and reviewing plans WITH the human
2. That collaborative time belongs in a **conversation** (chat), not in issue comments
3. But a single chat session isn't ideal either — you need adversarial reviews and iterations (which the issue-based flow provided via architect + lead reviewer hats)

**Proposed approach — GSD-style in-conversation review:**
When planning artifacts are produced in a chat session, spawn review agents (adversarial reviewers, like GSD's plan-checker) within the same session. They review, critique, and the human iterates — all in the conversation.

**Alternative — PR-based review:**
Once an artifact is drafted, open a PR. Review agents review on the PR. Pros: durable, visible, standard review tooling. Con: you lose the quick back-and-forth of chat.

**But these aren't mutually exclusive:**
The human can always start a chat session referencing a PR ("let's talk about this PR") to get the conversational back-and-forth. So the flow could be:
1. Chat session: collaborative planning, draft artifacts
2. PR opened with artifacts
3. Review agents review on the PR
4. Human can either respond on the PR OR start a new chat to discuss

**Key insight:** The chat is where high-bandwidth collaboration happens. The PR/issue is where artifacts land durably. The system should support moving between them fluidly.

### Q9: Where do planning artifacts live?

**A9:** Three-layer storage with traceability:

1. **Project repo** — artifacts are committed to the project repository (e.g., `specs/<epic-name>/design.md`, `specs/<epic-name>/requirements.md`). This is the source of truth. Version-controlled, PRable, diffable.

2. **Team repo** — points to where each project's artifacts live (e.g., "botminter project plans are at `specs/` in the botminter repo"). This is a static pointer per project, NOT an index that gets updated every time artifacts are added. The team repo just tells the agent "for project X, go look here."

3. **GitHub issue** — links to the artifacts in the project repo. The issue is the work item; the artifacts are in the repo. Cross-linked.

**Three ways an agent can find planning artifacts:**
- From the workspace: knows directly where the project's plans live (built-in convention)
- From the team repo: central index → traces to project plans
- From the issue: links in the issue body point to artifacts

**Belt and suspenders** — multiple paths to the same artifacts ensure the agent can always find them regardless of entry point.

### Q7: How are agent-internal implementation tasks externalized?

**A7:** Three modes for agent-internal task tracking:

1. **Off** — tasks stay inside the agent's context only. Not on GitHub at all.
2. **Sub-tasks** — tasks are tracked inside the parent story on GitHub, but NOT as separate issues. Could be a comment that gets updated, or a folded section in the story body. Exact mechanism TBD — this is implementation detail, not requirements. The point: durable and visible on the story, but no new issues created.
3. **Tasks** — full GitHub issues created for each agent-internal task. Labeled as agent-internal. NOT shown in the default human board view out of the box (may be in a separate view tab). User can later tweak their board to show them if they want.

Configurable at profile level and per-work-item level.

### Q10: Should BotMinter define a fixed artifact set, or make the planning methodology pluggable?

**A10:** Pluggable. BotMinter should NOT hardcode a specific artifact set (e.g., "planning always produces a design doc, ADRs, and an implementation plan"). Instead:

- **BotMinter defines a contract** — what it needs from any planning methodology in order to proceed through the SDLC (the two touch points: specs in, verification out).
- **GSD, PDD, or custom methodologies are implementations** of that contract. They produce whatever artifacts they produce, but they must satisfy BotMinter's interface.
- **Users can choose their methodology:**
  - Use GSD out of the box (if compatible)
  - Use PDD out of the box (if compatible)
  - Use something completely different or build their own
- **Key design questions:**
  - Can GSD and PDD be used as-is, or do they need a BotMinter-compatible adapter?
  - What does the abstraction/interface look like?
  - What does BotMinter actually need from a planning methodology to function?

**A10 (continued) — Layer clarification:**

Planning methodology is **not a BotMinter concern and not a profile concern** — it's a third layer:

1. **BotMinter (infrastructure)** — Team, Member, Formation, Bridge, Workspace, Daemon, Brain, Credentials. Runtime infrastructure. Knows nothing about process or methodology.

2. **Profile (methodology template)** — PROCESS.md, roles, statuses, labels, skills, knowledge, invariants. Defines the SDLC lifecycle: what work item types exist, what statuses they pass through, who does what. Does NOT implement planning — defines where planning sits in the lifecycle.

3. **Planning methodology (pluggable skill/plugin)** — GSD, PDD, custom. Produces artifacts. Satisfies whatever contract the profile requires for "planning is done, implementation can start."

The profile says "epics pass through a planning phase" and "stories need specs before implementation." The planning methodology is what actually does the planning work.

**What BotMinter currently hardcodes that it shouldn't:**
- GitHub as code host → should be a "hub" or "code host" abstraction
- GitHub Projects v2 as board → should be a "board" abstraction
- GitHub App as member identity → should be an "identity provider" abstraction

These are relevant because planning methodology needs to interact with the board (status transitions) and the code host (artifact storage in repos). If those aren't abstracted, the planning methodology can't be truly pluggable either.

### Q11: What's the right abstraction model — where does planning methodology live?

**A11:** Team plugins. The model is:

**BotMinter core** = managing teams, members, team infrastructure, member infrastructure (formation, secrets, identity). Teams and members have lifecycles.

**Team plugins** = pluggable extensions that hook into the team and member lifecycles. A plugin provides:
- **Prompts/prose** — content that gets woven into member prompts (e.g., references to GitHub statuses, or references to where GSD plans live)
- **Skills** — agent-consumable tools (e.g., `github-project` skill, or `gsd:plan-phase` skill)
- **Lifecycle hooks** — code that runs during CLI commands (e.g., `bm teams sync` pushes to git for GitHub-based teams, pushes to mercurial for mercurial-based teams)
- **CLI extensions** — new commands a profile can add (e.g., `bm plan` that starts a specific member with a specific skill)
- **Credential requirements** — creds initialized during `bm hire`, delivered during `bm start`
- **Status surface** — extra info shown during `bm status`

**Examples of team plugins:**
- **GitHub plugin** — provides GitHub-specific prompts, skills (github-project, board-scanner), credential flow (GitHub App identity), status display (issue counts, PR status), lifecycle hooks (label/project bootstrapping during `bm init`, workspace sync via git)
- **GSD plugin** — provides GSD-specific prompts (where plans live, artifact conventions), skills (plan-phase, discuss-phase, verify-work, execute-phase), possibly CLI extensions (`bm plan`)
- **PDD plugin** — provides PDD-specific prompts, the PDD skill
- **A hypothetical Linear plugin** — provides Linear-specific prompts, skills, credential flow (API tokens), status display

**Profile's role** = a profile selects and configures plugins. A profile says "this team uses GitHub + GSD" or "this team uses GitLab + PDD" or "this team uses GitHub + custom planning."

**Key architectural properties:**
1. Plugins compose — GitHub plugin + GSD plugin can coexist without conflict
2. Plugins are profile-selected — the profile declares which plugins are active
3. Plugins hook into BotMinter's lifecycle — they don't replace it, they extend it
4. Different profiles with different plugin selections behave differently for the same `bm` commands

**Scope for this plan:** Not all of the plugin architecture needs to be designed now. The planning plugin (GSD/PDD as first POC) is the immediate deliverable. The broader plugin system is captured as the target architecture, and the planning plugin should be designed to be compatible with it.

**What this means for our earlier questions:**
- Q9 (artifact storage) → the planning plugin decides where artifacts live in the project repo. The GitHub plugin handles the git push. The profile wires them together.
- Q7 (agent-internal tasks) → the GitHub plugin provides the mechanism for creating/tracking issues. The planning plugin decides whether to use it.
- Q8 (planning modes) → the planning plugin owns the logic. The profile provides the signals (labels, fields) that the plugin reads.

### Q12: What is the plugin interface — does every plugin have the same surface area?

**A12:** No. Plugins differ in what they provide. There is no uniform plugin contract with mandatory surface areas.

A planning plugin like GSD might just provide **skills** — and that's it. The profile's templating system does the integration:
- If the active planning plugin is GSD → the profile templates include GSD-specific text in prompts (where plans live, artifact naming conventions, how to invoke GSD skills)
- If the active planning plugin is PDD → different text, different skill references

The profile is the glue layer. It knows which plugins it supports and conditionally renders prompts, process docs, and skill references based on what's active.

**This means:**
- No formal Plugin trait with mandatory methods
- A plugin is just a directory of skills (+ maybe knowledge files)
- The profile declares supported plugins and templates around them
- Different plugins provide different things — some provide skills, some provide lifecycle hooks, some provide both. The profile knows what each one offers.

### Q13: PDD as first POC — how do plugin skills get wired into member workspaces?

**A13:** Use PDD (not GSD) as the first plugin POC. Which members get which skills is a profile decision.

**The wiring problem:** The plugin provides skills. The profile knows which members should get them. But how do the skills physically end up in the right workspace?

**Answer — two-phase team creation:**

1. **Deterministic phase** (current `bm init`) — instantiate the profile template into a team repo. Mechanical extraction that already exists. Produces the base team structure.

2. **Agentic phase** (new) — after deterministic extraction, launch a coding agent session that:
   - Knows what profile was used
   - Knows what plugins were selected
   - Knows what the resulting team repo should look like (goals, not steps)
   - Customizes the team repo: wires skills into the right members, adjusts prompts, sets up plugin-specific conventions
   
3. **Then `bm teams sync -a`** — the mechanical infrastructure (workspace provisioning, credential delivery, file surfacing) kicks in as it does today.

**Flow:**
```
bm init (deterministic profile extraction)
  → agentic session (plugin customization — goal-oriented, not scripted)
    → bm teams sync -a (mechanical provisioning)
```

**Why agentic, not deterministic?** Plugin wiring involves judgment calls — which prompts to modify, how to integrate skills with existing role context, how to handle conflicts between plugins. An agentic session can reason about these, whereas deterministic templating would require anticipating every combination.

**This approach was explored before** by the operator and is being revisited as the right fit for plugin integration.

### Q14: What triggers and runs the agentic session?

**A14:** Interactive onboarding — Minty.

After `bm init` does the deterministic extraction, an interactive agentic session starts. This is a conversational onboarding experience:

> "Welcome, I'm Minty. I'm here to help you set up and get onboarded with your team."

Minty knows:
- What profile was selected
- What plugins are available for that profile
- What the team repo looks like after deterministic extraction

Minty asks the operator about preferences, plugin choices, and wires everything up interactively. The operator can say "I want PDD for planning" and Minty handles the rest — wiring skills to the right members, adjusting prompts, setting up conventions.

This is a concierge, not a script. It reasons about the operator's choices and produces a properly configured team repo. When done, `bm teams sync -a` picks up the result mechanically.

### Q15: What does the PDD plugin look like for the agentic-sdlc-minimal profile?

**A15:** PDD skill stays as-is (monolithic, not broken up). The rest of the design is profile-specific.

**Profile-specific decisions for agentic-sdlc-minimal:**

1. **Onboarding skill** — the profile ships with an onboarding skill that Minty invokes during interactive team setup. This handles plugin wiring, including PDD.

2. **CLI extension** — the profile extends the CLI with a `bm plan` command. This starts a **planning session** with the engineer wearing the `lead_planner` hat. This is the interactive/collaborative planning path — the operator and the engineer do PDD together.

3. **Hat rename** — `arch_designer` is renamed to `lead_planner`. The planner leads the planning work (requirements, research, design, implementation plan via PDD).

4. **Adversarial review is built into the planning skill, not a separate hat handoff:**

   Both paths use the same review mechanism: after each artifact is produced, the skill spins up **3 adversarial `arch_reviewer` agents** that try to poke holes. This happens inside the planning session itself.

5. **Two planning paths — same skill, different entry point:**

   | Path | Entry point | Planning | Review |
   |------|------------|----------|--------|
   | **Interactive** | `bm plan` → operator + engineer (`lead_planner` hat) | PDD skill, conversational with operator | Skill spins up 3 adversarial `arch_reviewer` agents after each artifact |
   | **Autonomous** | Epic hits planning status → Ralph loop | `lead_planner` hat invokes PDD skill (same skill, non-conversational) | `arch_reviewer` hat spins up 3 adversarial review agents |

   The interactive path has the operator in the loop during PDD. The autonomous path uses the same PDD skill but through Ralph hat transitions: `lead_planner` hat produces artifacts → `arch_reviewer` hat reviews with adversarial agents.

   **Key:** Both paths produce the same artifacts and use the same adversarial review. The difference is whether the operator is present during production.

6. **`lead_reviewer` → `arch_reviewer`** — the reviewer hat is renamed to reflect that it's an architectural review of the plan, not a lead-level review.

7. **Review feedback handling** — follows standard Ralph review patterns:
   - **Autonomous:** reviewer emits a rejected event (same as existing engineer hat review flow). `lead_planner` iterates.
   - **Interactive:** feedback is presented to the human, who decides whether to address it or move on.

8. **Adversarial reviewer perspectives** — each of the 3 `arch_reviewer` agents reviews from a **different perspective**, and the perspectives vary based on which artifact is being reviewed. For example:
   - Design doc → architecture perspective, security perspective, maintainability perspective
   - Requirements → completeness perspective, feasibility perspective, testability perspective
   - Implementation plan → scope perspective, dependency correctness perspective, risk perspective
   
   The specific perspectives per artifact type are a design decision to be defined in the detailed design phase.
   
   Reference: GSD's plan-checker uses 10 fixed dimensions. GSD's cross-AI peer review uses independent models. Our approach is a hybrid — multiple reviewers, each with a distinct angle, tailored to the artifact.

### Q18: Is verification/acceptance part of the PDD plugin POC?

**A18:** Yes. Verification/acceptance must be part of the first deliverable. PDD currently has no verification layer — that's a GSD strength. We need to create a PDD-native verification mechanism inspired by GSD's verify-work.

**GSD's verify-work for reference:**
- Extracts testable deliverables from SUMMARY.md files
- Presents tests one at a time to the human
- User responds naturally; severity inferred (crash=blocker, doesn't work=major, etc.)
- On failures: auto-diagnosis via debug agents, auto fix-plan creation
- Produces UAT.md with gaps that feed back into planning

**What the PDD verification parallel needs to do:**
- Satisfy the second human touch point (the "here's what you got" handoff)
- Support both gated (human approves) and post-hoc awareness (human catches up) modes
- Active evaluation, not passive notification (from Q2)

### Q19: What are the verification criteria for PDD? Do we need a new format?

**A19:** No new format needed. PDD's existing **acceptance criteria** (Given-When-Then) ARE the verification criteria.

BotMinter has used PDD heavily — four full PDD projects in the repo history. All produce acceptance criteria in GWT format that are already:
- Concrete and observable (specific inputs → specific outcomes)
- Organized by subsystem/component
- Traceable to requirements
- Directly mappable to automated and manual tests

Examples from `specs/github-app-identity/design.md` (27 criteria):
```
Given `bm hire <role> --name superman`, when the operator completes the
manifest flow, then a GitHub App named `{team}-superman` is created,
credentials stored in keyring, App installed on team repo + project repos.
```

These are functionally equivalent to GSD's `must_haves.truths`. The format differs (GWT prose vs YAML), but the content is the same: observable behaviors from the user's perspective that define "done."

**The verification flow becomes:**
1. PDD planning produces acceptance criteria (GWT) as part of the design doc
2. After implementation, the verification step walks through these criteria
3. Each criterion is checked — automated where possible, human where needed
4. Failures feed back as gaps

### Q20: Verification details — IDs, automated+UAT flow, gap capture

**A20:** The plugin is not just PDD — it's the **full Agent SOP pipeline**: PDD → code-task-generator → code-assist. All three skills working together. PDD produces design + plan, code-task-generator breaks plan steps into `.code-task.md` files, code-assist implements each task via TDD.

**Current state of these skills:**
- PDD: no IDs, no acceptance criteria mandate, no traceability
- Code-task-generator: has GWT acceptance criteria format but no IDs, no traceability
- Code-assist: TDD implementation, no traceability back to requirements

**Enhancement — IDs and traceability across the full pipeline (inspired by GSD):**

**1. Everything that can be catalogued gets an ID:**
- Questions: `Q-NN` (already exists informally in idea-honing)
- Requirements: `CATEGORY-NN` (AUTH-01, FORM-02) — category abbreviation (3-5 uppercase chars) + zero-padded sequential number
- Acceptance Criteria: `AC-NN` (AC-01, AC-02) — or `CATEGORY-AC-NN` for category-scoped
- Decisions: `D-NN` (D-01, D-02)
- Research topics: `R-NN` (R-01, R-02)
- ADRs: `ADR-NNNN` (ADR-0008) — already established in BotMinter
- Implementation steps: `STEP-NN` (STEP-01, STEP-02)

**2. Traceability flows through the pipeline:**
- PDD Step 6 (Design): requirements get `CATEGORY-NN`, acceptance criteria get `AC-NN`, traceability matrix at end
- PDD Step 7 (Plan): each step references which requirement IDs it addresses
- Code-task-generator: carries requirement IDs and AC IDs into task files
- Verification: checks against AC IDs after implementation

**Open question for design phase — Agent SOP pipeline vs SDLC hierarchy and engineer hats:**
- How do PDD / code-task-generator / code-assist map against the epic → story → task hierarchy?
  - PDD produces a plan (epic-level?) whose steps become stories? And code-task-generator breaks those into tasks?
  - Or does PDD run at story-level too, with code-task-generator producing tasks directly?
- Code-assist is a TDD implementation skill. The engineer already has implementation hats (developer, QE). Does code-assist replace those? Augment them? Conflict?
- How does the existing Ralph hat workflow (developer hat implements, QE hat tests, lead reviews) interact with code-assist's built-in Explore → Plan → Code → Commit cycle?
- Which skills go to which hats? `lead_planner` gets PDD + code-task-generator? Developer hat gets code-assist? Or does the engineer get all of them and the hat context determines which to invoke?

These questions require understanding the full interaction between Agent SOP skills and BotMinter's role/hat model — deferred to the design phase.

### Q21: How does planning differ by work item level?

**A21:** Two non-negotiable requirements:

**1. Friction-free entry at any level.** The user must be able to start at any level (epic, story, task) without being forced through the full epic ceremony. This is a first-hand pain point — dreading the full epic flow when you just want to implement something simple kills adoption.

**2. No gaps in the project's source of truth.** Starting at a lower level must not create holes in the project's requirements, design docs, ADRs, scenarios, etc. If someone starts at story level and implements what turns out to be a new feature with a new design, the project should still end up with proper requirements and design documentation — not just an orphaned story with no upstream traceability.

**How to reconcile these:**

Each level should be **aware of when it's okay to be orphan and when it's not.** Two mechanisms:

1. **Nudge up** — if the system detects that a story-level entry is really epic-scope work (new feature, new architecture, multiple components), it nudges the user: "This looks like it needs a design doc. Want to start at epic level instead?"

2. **Work backwards** — give the user the option to leverage the AI to generate upstream artifacts retroactively. "You implemented this as a story, but there's no requirements doc for this feature. Want me to generate one from what was built?" This is NOT necessarily creating GitHub epics or issues — it's about filling gaps in the project's source of truth (requirements docs, design docs, ADRs, etc.).

**The direction of work is bidirectional:**
- **Top-down (normal):** epic → PDD → design + plan → stories → tasks → implement
- **Bottom-up (retroactive):** task/story implemented → system detects gaps → offers to generate upstream artifacts from what exists

**PDD's requirements document — split out from design doc:**
In current PDD, the idea-honing.md is raw Q&A; the "requirements" are consolidated into the design doc's "Detailed Requirements" section. There is no standalone requirements document.

**Decision:** Requirements MUST be a standalone document (`requirements.md`), not embedded in the design doc. This enables:
- Independent existence with `CATEGORY-NN` IDs for traceability
- Working backwards: requirements can be added retroactively without a full design doc
- Project-level accumulation across work items
- Design doc references requirements by ID instead of duplicating them

**Enhanced PDD artifact set:**
```
idea-honing.md            ← Q&A (Q-NN)
requirements.md           ← standalone, categorized IDs (AUTH-01, FORM-02)
research/                 ← research notes (R-NN)
design/detailed-design.md ← references requirements by ID, has acceptance criteria (AC-NN)
implementation/plan.md    ← steps reference requirement IDs (STEP-NN)
```

**Open questions for design phase:**
- What heuristics detect "this is bigger than the level you entered at"?
- What does "work backwards" produce — a full PDD design doc retroactively? Just a requirements summary? Just update the traceability?
- Should there be a project-level requirements registry that accumulates across all work items, regardless of entry level?
- Where do the retroactively generated artifacts live relative to the originals?

### Q22: How do Spike and Bug fit?

**A22:**

**Spike** — a work item meant to address specific questions. Can optionally produce artifacts (POC, research doc, etc.). Deferred for now — not in scope for this plan.

**Bug** — two tracks, minimal variation:

1. **Simple bug** — autonomous fix. No artifacts needed. Agent picks it up, fixes it, done. No planning ceremony.

2. **Complex bug** — instead of fixing it directly, the bug generates a **story**. The proposal is: "this bug is complex, let's create this story — when implemented, the bug goes away." The story then follows normal story-level planning (with whatever planning depth applies).

**Key simplification:** There is no "complex bug fix" track. A complex bug is just a bug that escalates to a story. This avoids having a separate planning process for bugs — bugs are either trivially fixed or become stories.

**This maps to the existing profile:** the current agentic-sdlc-minimal profile already has simple + complex bug paths. The redesign formalizes this: simple = auto-fix, complex = generate story.

### Deferred to design phase

The following topics are captured but deferred to the design phase for resolution:

1. **Planning mode signals** — what signals on a work item tell the autonomous agent which mode to use (full autonomous, produce-and-wait, wait-for-artifacts)? Labels, fields, profile defaults? The requirement is "the agent should not have to guess."

2. **Verification skill** — `bm verify` loads a persona and invokes the verification skill. Which hat/persona is the right one to verify? (Should not be the same one that planned or implemented.) What does the skill look like operationally?

3. **Agent SOP pipeline vs SDLC hierarchy** — how do PDD / code-task-generator / code-assist map against epic → story → task? Which skills go to which hats?

4. **"Work backwards" mechanism** — heuristics for detecting scope mismatch, what retroactive artifacts are generated, where they live.

5. **Gap capture details** — format, severity classification, how gaps feed back into the board.

6. **Adversarial reviewer perspectives** — specific perspectives per artifact type.

**2. Acceptance flow — automated verification then UAT:**
When acceptance starts, two sequential steps:
1. **Automated verification** — AI runs the "demos" from the implementation plan + structural checks. Produces a verification report.
2. **UAT** — human is presented with the automated verification report first. Any automated checks that failed, the human manually executes to confirm. Then walks through the GWT acceptance criteria conversationally.

**3. UAT is two-pair-of-eyes:**
AI performs each acceptance criterion check first, then asks the human to do the same and compare notes. Not just "show expected, ask if matches" — the AI actively tests and presents its findings alongside the human's assessment.

(Note: GSD's actual implementation is sequential — AI does structural checks, then human does behavioral UAT separately. Our approach combines them more tightly as two-pair-of-eyes per criterion.)

**4. Gap capture during UAT:**
During UAT, gaps are captured and categorized: future TODOs, bugs, improvements. The exact gap capture mechanism (format, how they feed back into the board, severity classification) will be designed in detail as a follow-up — for now the requirement is that the acceptance skill captures them, inspired by GSD's gap → diagnosis → fix-plan → re-verify cycle.
