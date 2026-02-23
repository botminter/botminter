# Design Principles

Validated rules for building member skeletons (`ralph.yml`, `PROMPT.md`, `CLAUDE.md`, hat instructions) and sprint-level prompts. These principles were established through the Milestone 1.5 (M1.5) spike, Milestone 2 (M2) design, and sprint execution.

## 1. Separation of concerns

| Layer | Purpose | Examples |
|-------|---------|---------|
| **PROMPT.md** | Role identity + cross-hat behavioral rules | "You are the architect", training mode declaration, sync-before-scan |
| **CLAUDE.md** | Role context — workspace model, references, invariant locations | Fork chain, CWD (Current Working Directory) model, pointers to `.botminter/CLAUDE.md` and `PROCESS.md` |
| **Hat instructions** (`ralph.yml`) | Operational details for each hat | Event publishing, poll-log, knowledge paths, backpressure gates |
| **`core.guardrails`** (`ralph.yml`) | Universal behavioral rules (numbered 999+) | Lock discipline, invariant compliance |

**Rules:**

- PROMPT.md and CLAUDE.md must not prompt about hats. Ralph handles hat prompting.
- Cross-hat concerns go in PROMPT.md. Hat-specific concerns go in hat instructions.
- Knowledge paths go in each hat's `### Knowledge` section — not in PROMPT.md or CLAUDE.md.
- Generic invariants go in CLAUDE.md's `# INVARIANTS` section.
- Hat-specific quality gates go in `### Backpressure` in hat instructions.
- Universal guardrails go in `core.guardrails` in ralph.yml.

## 2. Ralph runtime pattern

These behaviors were validated in the M1.5 spike:

| Rule | Rationale |
|------|-----------|
| Do not set `starting_event` | All routing must go through an orchestrator hat (board scanner). Without it, rejection loops and priority dispatch break. |
| Set `persistent: true` | Keeps the agent alive. `LOOP_COMPLETE` is suppressed; `task.resume` restarts the loop. |
| `task.resume` is reserved | Must not appear in hat triggers. The coordinator handles it. |
| Board scanner publishes `LOOP_COMPLETE` when idle | Signals no work found. |
| Work hats have `default_publishes` | Prevents fallback exhaustion for non-orchestrator hats. |
| Board scanner clears scratchpad and tasks at cycle start | Prevents context pollution between issues. |
| Idempotent dispatch | Verify issue is not already at target status before dispatching. |
| One event per hat trigger list | Duplicate events across hats cause ambiguous routing. |
| Do not set `cooldown_delay_seconds` | Agent processing time provides natural throttling. |

### `LOOP_COMPLETE` handling

- Must not appear in a hat's `publishes` list in ralph.yml
- Must only be referenced in the hat's `instructions` block
- Work hats use `default_publishes: LOOP_COMPLETE` as a safety net; the board scanner does not need it since it explicitly publishes `LOOP_COMPLETE` in its instructions when idle

### Two execution models

Members support two execution models controlled by the `event_loop.persistent` field in `ralph.yml`:

| Model | `persistent` | Entry point | Exit condition | Restart responsibility |
|-------|-------------|-------------|---------------|----------------------|
| **Poll-based** | `true` | Board scanner on `board.scan` | Never (`LOOP_COMPLETE` → `task.resume` → `board.scan`) | Self (Ralph runtime) |
| **Event-triggered** | `false` | Board scanner on `board.scan` | `LOOP_COMPLETE` after no work found | External (daemon) |

**Poll-based (default):** The board scanner runs in a loop. On `LOOP_COMPLETE`, Ralph injects `task.resume` which routes back to `board.scan`. The member stays alive indefinitely.

**Event-triggered:** The board scanner runs once (or until no work remains). It scans all matching issues — not just the first one. When no work is found, it publishes `LOOP_COMPLETE` and Ralph exits. The [daemon](../reference/cli.md#daemon) handles the next restart.

```yaml
event_loop:
  persistent: false              # event-triggered mode
  max_iterations: 200            # safety limit per invocation
```

The board scanner must process all matching work items in priority order before publishing `LOOP_COMPLETE`. The key invariant: **the board scanner is always the universal entry point, regardless of execution model.**

### Rejection routing

Rejection events go unmatched — no hat subscribes. The hatless Ralph orchestrator examines context, determines which work hat's output was rejected, and routes back to that hat directly.

Work hats must not subscribe to rejection events when multiple work hats share the same reviewer.

## 3. Human-in-the-loop (HIL)

| Rule | Detail |
|------|--------|
| Use `human.interact` without explicit two-pass logic | Hat instructions say "present/report via `human.interact`". Ralph handles blocking and response delivery transparently. |
| Training mode is universal | All team members, not just the human-assistant. |
| Training mode is a PROMPT.md toggle | Declared as `## !IMPORTANT — OPERATING MODE`. Hat instructions check it conditionally. |
| Training mode is a superset of poll-log | Training mode adds `human.interact` on top of operational logging. |

## 4. Operational logging

- Poll-log must not be gated by training mode — it is operational logging for debugging.
- Poll-log lives at workspace root (`poll-log.txt`) and is gitignored.

## 5. Workspace model rules

| Rule | Detail |
|------|--------|
| Agent CWD is the project repo | Team repo cloned into `.botminter/` inside it. |
| All roles use the same workspace model | Including non-code-working roles. |
| `.botminter/.member` marker required | Read by `bm teams sync` to identify the workspace member. |
| PROMPT.md and CLAUDE.md are symlinks | Auto-update on pull. |
| ralph.yml and settings.local.json are copies | Require `bm teams sync` + restart. |
| Skills read directly via `skills.dirs` | No assembly needed. |
| Agents symlinked into `.claude/agents/` | From all `agent/agents/` layers. |
| `.botminter/` excluded via `.git/info/exclude` and `.gitignore` | `bm teams sync` verifies and repairs. |

## 6. Knowledge, invariants, and backpressure

| Tier | Where configured | Scope | Rules |
|------|-----------------|-------|-------|
| **Knowledge** | `### Knowledge` in hat instructions | Per hat | Lazy — list directories, agent decides what is relevant. Do not force upfront reads. |
| **Invariants** | `# INVARIANTS` in CLAUDE.md | All hats | Deep context, file-based, accumulated across scopes. |
| **Backpressure** | `### Backpressure` in hat instructions | Per hat | Short, verifiable conditions that block status transitions. Define what, not how. |
| **Guardrails** | `core.guardrails` in ralph.yml | All hats | Universal rules injected as numbered `### GUARDRAILS` into every hat prompt. |

## 7. Sprint PROMPT.md design

Sprint PROMPTs drive Ralph to implement a sprint's work. They follow a standard skeleton:

```
# Sprint N: Title
## Objective          — 1-2 sentences. What, not how.
## Prerequisites      — What the prior sprint delivered.
## Deviations         — What is intentionally out of scope vs the design.
## Key References     — Pointers to design.md, sprint plan, research.
## Requirements       — Numbered list. WHAT changes, not HOW.
## Acceptance Criteria — Given-When-Then quality gates.
```

### Sprint PROMPT rules

- Requirements state WHAT and reference WHERE — do not duplicate design content
- Do not prescribe hat instruction text — describe the gate/behavior
- Do not prescribe implementation steps — state the outcome
- Use RFC 2119 language (MUST, SHOULD, MAY) consistently
- Deviations from design must be explicit with rationale
- Sprints chain via prerequisites — each session starts fresh
- Regression criteria are prefixed with "(Regression)"

### Anti-patterns

| Anti-pattern | Fix |
|-------------|-----|
| Pasting design content into requirements | Reference the section: "Per design.md Section X" |
| Quoting exact hat instruction text | Describe the gate/behavior |
| Step-by-step implementation sequence | State the outcome |
| Vague "add X" without stating current state | Acknowledge what exists |
| Casual language for hard constraints | Use MUST/MUST NOT |

## Related topics

- [Configuration Files](configuration.md) — ralph.yml, PROMPT.md, CLAUDE.md structure
- [Knowledge & Invariants](../concepts/knowledge-invariants.md) — recursive scoping model
- [Member Roles](member-roles.md) — role-specific configurations
