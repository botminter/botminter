# Ralph Orchestrator: Events Evidence & the Confession Phase

> How Ralph validates agent claims of completion — quality gates, evidence thresholds, and confidence-aware self-assessment.

## TL;DR

**Agents can't just say "done."** When an agent emits certain event topics (`build.done`, `review.done`, `verify.passed`), Ralph parses the event payload for structured evidence. If evidence is missing or checks are failing, the event is **rewritten** to a blocked/failed variant and the agent must retry. This is called **backpressure** — Ralph doesn't prescribe how to work, it just rejects incomplete work.

On top of this, a **Confession phase** adds a second layer: semantic self-assessment via hat instructions, where agents are incentivized to find and report their own mistakes before the loop can complete.

---

## The Three Evidence Gates

Ralph enforces evidence on exactly three event topics. Every other topic passes through unchecked.

### 1. Build Evidence — Guards `build.done`

When an agent claims the build is done, it must include proof in the event payload. Ralph scans the payload text for specific key-value pairs.

#### Required checks (must be present and passing)

| Check | What to include in payload | If missing or failing |
|-------|---------------------------|----------------------|
| Tests | `tests: pass` | Blocks |
| Lint | `lint: pass` | Blocks |
| Typecheck | `typecheck: pass` | Blocks |
| Audit | `audit: pass` | Blocks |
| Coverage | `coverage: pass` | Blocks |
| Complexity | `complexity: 7` (any number ≤ 10) | Blocks |
| Duplication | `duplication: pass` | Blocks |

#### Optional checks (only block when explicitly negative)

| Check | What to include | Behavior |
|-------|----------------|----------|
| Performance | `performance: pass` | Omitting is fine. Reporting a regression blocks. |
| Specs | `specs: pass` | Omitting is fine. Reporting `specs: fail` blocks. |
| Mutation testing | `mutants: pass (82%)` | **Warning-only** — never blocks `build.done`. |

**What "required" vs. "optional" means in practice:** If you omit a required field entirely, Ralph treats it as failed — the build is blocked. If you omit an optional field, Ralph assumes it's not applicable and lets the event through. But if you *include* an optional field with a negative result (e.g., `specs: fail` or reporting a performance regression), that explicitly blocks. This design lets teams gradually adopt new quality gates without breaking existing workflows.

#### A valid `build.done` payload looks like:

```
ralph emit "build.done" "tests: pass, lint: pass, typecheck: pass, audit: pass, coverage: pass, complexity: 5, duplication: pass"
```

Or multi-line:

```
ralph emit "build.done" "tests: pass
lint: pass
typecheck: pass
audit: pass
coverage: pass
complexity: 5
duplication: pass
specs: pass
performance: pass"
```

Both formats work — Ralph splits on newlines or commas.

### 2. Review Evidence — Guards `review.done`

A simpler gate. A reviewer hat must prove it ran verification, not just eyeballed the code.

| Check | What to include | Required? |
|-------|----------------|-----------|
| Tests | `tests: pass` | Yes |
| Build | `build: pass` | Yes |

Both must be present and passing, or `review.done` becomes `review.blocked`.

```
ralph emit "review.done" "tests: pass, build: pass"
```

### 3. Quality Report — Guards `verify.passed`

The most metrics-rich gate, used by verifier hats. This one has **numeric thresholds** — not just pass/fail booleans.

| Dimension | Payload format | Threshold |
|-----------|---------------|-----------|
| Tests | `quality.tests: pass` | Must pass |
| Lint | `quality.lint: pass` | Must pass |
| Audit | `quality.audit: pass` | Must pass |
| Coverage | `quality.coverage: 82%` | **>= 80%** |
| Mutation | `quality.mutation: 71%` | **>= 70%** |
| Complexity | `quality.complexity: 7` | **<= 10.0** |
| Specs | `quality.specs: pass` | Optional; explicitly failing blocks |

```
ralph emit "verify.passed" "quality.tests: pass, quality.lint: pass, quality.audit: pass, quality.coverage: 85, quality.mutation: 75, quality.complexity: 6"
```

When a `verify.passed` event fails thresholds, Ralph rewrites it to `verify.failed` and includes a message listing exactly which dimensions failed (e.g., "quality thresholds failed: coverage, mutation"). This tells the agent what to fix.

---

## How Ralph Decides: The Evidence Flow

When an agent emits an event via `ralph emit`, the orchestrator processes it before publishing to the event bus:

```
Agent emits event
      |
      v
Is topic "build.done"? ──Yes──> Parse payload for build evidence
      |                              |
      | No                     Evidence found?
      v                         |          |
Is topic "review.done"? ──>   No          Yes
      |                        |           |
      | No                     v      All checks pass?
      v                   Synthesize    |        |
Is topic "verify.passed"? ──>  blocked  Yes      No
      |                        event     |        |
      | No                              v        v
      v                           Accept    Synthesize
Accept event as-is               event     blocked event
```

**The key mechanism: event transformation.** Ralph never silently drops a failed event. Instead, it **rewrites the topic** to a blocked/failed variant:

| What the agent emitted | What Ralph publishes instead | Why |
|------------------------|------------------------------|-----|
| `build.done` (evidence failing) | `build.blocked` | Checks didn't pass |
| `build.done` (no evidence at all) | `build.blocked` | Evidence was missing entirely |
| `review.done` (evidence failing) | `review.blocked` | Tests or build not verified |
| `verify.passed` (thresholds not met) | `verify.failed` | Numeric thresholds not reached |

The original event **never reaches the event bus**. Any hat subscribed to `build.done` will never see a blocked build. Instead, hats listening for `build.blocked` handle the remediation — or the same hat gets re-invoked to fix its work.

The payload of the synthesized blocked event is a human-readable instruction telling the agent exactly what went wrong. This message becomes part of the agent's next prompt context, so the agent knows what to fix.

### What bypasses the gates

Not everything goes through evidence validation:

- **Custom topics** like `feature.done`, `bugfix.done`, or `confession.clean` pass through with **zero validation**. Only the three exact topic strings above are gated.
- **`default_publishes` events** — if a hat has `default_publishes: "build.done"` and the agent fails to emit any event, Ralph injects a `build.done` with an empty payload **directly into the bus**, bypassing evidence checks entirely.
- **Internal orchestrator events** like `task.resume`, `task.start`, or hat exhaustion events are injected directly and never validated.

**Topic name precision matters.** `build.done` hits the gate; `build.complete` passes through unchecked. `verify.passed` is gated; `verify.pass` is not. One-word differences are the difference between enforced quality and a free pass.

---

## Who Determines the Thresholds?

Three levels of authority, from hardest to softest:

### 1. Ralph itself (non-configurable)

Three numeric thresholds are baked into Ralph and cannot be changed via configuration:

- **Coverage:** >= 80%
- **Mutation:** >= 70%
- **Complexity:** <= 10.0

These apply to both build evidence (complexity only) and quality reports (all three). They represent a baseline quality floor that every Ralph deployment enforces identically.

### 2. Your `ralph.yml` (configurable, warning-only)

One threshold is configurable — the **mutation score warning threshold**:

```yaml
event_loop:
  mutation_score_warn_threshold: 80.0  # Optional, 0-100
```

This is **advisory only** — it logs a warning in diagnostics but does NOT block `build.done`. It's useful for surfacing mutation score degradation trends before they become a problem at the `verify.passed` gate.

### 3. The agent (payload author)

Agents determine the actual values by running checks and reporting results in their event payloads. Ralph doesn't run tests, lint, or any tools itself — it only **validates what agents report**. This is a trust-but-verify model: Ralph trusts that agents report honestly, but applies structural verification so agents can't skip the checklist.

---

## How Ralph Parses Evidence

Understanding the parsing helps you write payloads that pass:

- **ANSI codes are stripped.** If your agent's tool output includes terminal colors, Ralph strips them before parsing. You don't need to worry about color codes breaking evidence detection.
- **Pass/fail is exact string matching.** Ralph looks for the literal substring `tests: pass` — not `tests: passed`, not `tests: PASS`, not `tests: true`. The colon-space-pass format is required.
- **Numbers are extracted flexibly.** For complexity and percentages, Ralph extracts the first number it finds on the relevant line. `quality.coverage: 82%`, `quality.coverage: 82`, and `quality.coverage: achieved 82% coverage` all work.
- **Evidence presence is detected first.** If Ralph finds zero evidence keywords in the payload (no `tests:`, no `lint:`, nothing), it treats the entire payload as "no evidence provided" — a different error than "evidence provided but failing."
- **Comma-separated and multi-line both work.** Ralph splits the payload on both newlines and commas, so `tests: pass, lint: pass` and a multi-line format are equivalent.

---

## The Mutation Warning Subsystem

Mutation testing occupies a unique position — it's the only dimension that exists in **warning-only** mode for builds but **hard-gate** mode for verification.

When a `build.done` passes all evidence checks, Ralph additionally inspects mutation evidence (if present) and may log warnings:

| Mutation status in payload | What Ralph does |
|---------------------------|-----------------|
| `mutants: fail` | Logs warning (does NOT block) |
| `mutants: warn` | Logs warning with score |
| `mutants: pass (65%)` with threshold configured at 80% | Logs "mutation score 65% below threshold 80%" |
| `mutants: pass (90%)` | No warning |
| No mutation evidence, but threshold configured | Logs "missing mutation evidence" |
| No mutation evidence, no threshold configured | Silent — nothing happens |

This graduated model gives builders early feedback about mutation score trends without blocking their work. The hard enforcement happens later when a verifier hat emits `verify.passed` — at that point, the 70% floor is enforced mechanically.

To enable mutation warnings, add to your `ralph.yml`:

```yaml
event_loop:
  mutation_score_warn_threshold: 80.0
```

You can observe these warnings in diagnostics output (`RALPH_DIAGNOSTICS=1`).

---

## The Confession Phase

### Background

Inspired by [OpenAI's Confessions research](https://alignment.openai.com/confessions/), the confession phase adds a confidence-aware loop completion mechanism. It was proposed in [issue #74](https://github.com/mikeyobrien/ralph-orchestrator/issues/74) and designed to work entirely through hat instructions — no changes to Ralph's core engine.

### The Core Idea

**Decouple usefulness from honesty.** The main answer is optimized for solving the user's task. A secondary output (the "ConfessionReport") is optimized for transparency, self-critique, and uncertainty disclosure. Only the ConfessionReport determines whether to accept, retry, or escalate.

This matters because LLMs tend to optimize for appearing helpful — which can mean hiding uncertainty or glossing over shortcuts. By creating a separate phase where the incentive is reversed ("you're rewarded for finding problems"), you get more honest quality signals.

### How It Works in Ralph

The confession pattern is implemented as a three-hat pipeline:

```
build.task --> [Builder] --> build.done --> [Confessor] --> confession.{clean,issues_found}
                                                                |
                                                        [Confession Handler]
                                                          |            |
                                                    build.task    LOOP_COMPLETE
                                                   (retry loop)    (accept)
```

#### The Builder

Triggers on `build.task`, publishes `build.done`.

The builder works normally but is instructed to maintain an **"Internal Monologue"** section in the scratchpad as it works — documenting shortcuts taken, things it's unsure about, assumptions made, edge cases skipped, and what it would do differently with more time. This externalized reasoning is critical: since Ralph uses fresh context per iteration, the Confessor can't share the Builder's "thought process" directly. The monologue on disk is the bridge.

#### The Confessor

Triggers on `build.done`, publishes `confession.clean` or `confession.issues_found`.

The Confessor's instructions explicitly state it is **rewarded only for finding issues**, not for saying the work is good. It acts as an "anonymous tip line." It reads the Builder's Internal Monologue, the actual code changes, and the original requirements, then produces a ConfessionReport:

- **Objective assessment:** For each objective the work should satisfy — was it met? What's the evidence? Confidence score (0-100%)?
- **Uncertainties and shortcuts:** What was assumed, omitted, or hacked?
- **Single Easiest Issue to Verify:** One concrete, testable claim — a specific command to run, a file to check. This is the trust-calibration anchor.
- **Decision:** If ANY issues found OR confidence < 80 → publish `confession.issues_found`. Only if genuinely nothing (and that's rare) → `confession.clean`.

The key insight from the OpenAI research: *"It is easier to verify a single thing the model claims to have done wrong, than to find and verify all the potential issues."* The Confessor only needs to find ONE real issue to be useful. Fabricating a false "all clear" requires elaborate, consistent lying across multiple dimensions — which is harder than just telling the truth.

#### The Confession Handler

Triggers on `confession.clean` or `confession.issues_found`, publishes `build.task` or `escalate.human`.

The Handler uses the "Single Easiest Issue to Verify" as a trust calibration step:

1. Run the verification from the confession (e.g., a specific test command, checking a specific file)
2. If the reported issue is real → the confession is trustworthy → review all confessed issues → either retry with specific fixes (`build.task`) or escalate for major issues (`escalate.human`)
3. If the reported issue is NOT real → the confession may be unreliable → request a new confession
4. If `confession.clean` → be skeptical (finding nothing is itself suspicious) → verify at least one positive claim
5. Only if clean AND confidence >= 80 → emit `LOOP_COMPLETE`

### Why This Is a Preset, Not an Engine Feature

This is a deliberate architectural choice. Ralph's design philosophy (Tenet #1: *"Anti-pattern: Building features into the orchestrator that agents can handle"*) means the confession phase lives entirely in hat configuration. The event bus already supports arbitrary topic names — `confession.clean` and `confession.issues_found` work out of the box because topic routing is topic-agnostic.

The 80% confidence threshold lives in the hat's instructions as natural language, not as a hard-coded constant like the coverage or mutation thresholds. This makes it flexible (teams can adjust the threshold in their preset YAML) but also softer (an agent could misinterpret or ignore it).

---

## Evidence vs. Confession: Two Layers of Trust

These two systems are complementary, not competing. They operate at different layers:

| Dimension | Backpressure Evidence | Confession Phase |
|-----------|----------------------|------------------|
| **What it checks** | Mechanical quality — did tests pass? Is lint clean? Is complexity reasonable? | Semantic quality — did we solve the right problem? Are we being honest about gaps? |
| **Trust model** | Structural verification — "did you include the required fields?" | Behavioral incentive — "you're rewarded for finding issues" |
| **Enforced by** | Ralph's event loop engine (deterministic, cannot be bypassed) | Hat instructions (prompt-driven, relies on LLM following instructions) |
| **Threshold location** | Baked into Ralph (80% coverage, 70% mutation, 10.0 complexity) | In the preset's hat instructions ("confidence >= 80") |
| **What it gates** | Event acceptance — `build.done` either passes or becomes `build.blocked` | Loop completion — the loop either retries or emits `LOOP_COMPLETE` |
| **On failure** | Event topic is rewritten (deterministic) | `build.task` is re-published (agent-driven) |

In the confession-loop pipeline, both layers activate in sequence:

1. The Builder emits `build.done` with backpressure evidence
2. Ralph's engine validates that evidence — if complexity > 10 or tests didn't pass, `build.blocked` fires and the Confessor **never runs**
3. Only after backpressure passes does the Confessor receive `build.done`
4. The Confessor then applies semantic quality assessment on top

This means backpressure is the **floor** — it catches objective, measurable failures. The confession is the **ceiling check** — it catches subjective issues like "we solved the wrong problem" or "we made assumptions we shouldn't have."

---

## Practical Considerations

### Writing hat instructions for evidence

If your hats publish `build.done`, `review.done`, or `verify.passed`, include the exact payload format in your hat's `instructions` field. Don't rely on the agent figuring it out from the generic event-writing example in the prompt — be explicit:

```yaml
hats:
  builder:
    name: "Builder"
    triggers: ["build.task"]
    publishes: ["build.done"]
    instructions: |
      After all checks pass, emit with evidence:
      ralph emit "build.done" "tests: pass, lint: pass, typecheck: pass, audit: pass, coverage: pass, complexity: <score>, duplication: pass"
```

### Funneling custom work through evidence gates

If you have domain-specific hats (feature builder, bugfixer, docs writer), you must decide whether they publish through a gated topic or a custom one:

- **`build.done`** → gets backpressure validation (recommended for code work)
- **`verify.passed`** → gets quality report validation with numeric thresholds (good for verification phases)
- **`feature.done`** or any custom topic → passes through unchecked (total freedom, but no safety net)

### Using `verify.passed` for non-code reviews

The quality report dimensions are abstract enough to map to any review domain. The parser only checks for `quality.*:` key-value pairs and numeric thresholds — it doesn't care what the dimensions *mean*. You redefine the semantics in your hat's instructions. For example, a docs reviewer can map `quality.tests` to "accuracy," `quality.coverage` to "section coverage," and `quality.complexity` to "readability score."

### The "Internal Monologue" convention

If you're building a confession-style pipeline, the Builder's Internal Monologue is the critical bridge between hats. Without it, the Confessor is limited to reading code output without knowing *why* decisions were made. Instruct your Builder hat explicitly:

```yaml
instructions: |
  As you work, maintain a running "Internal Monologue" section in the scratchpad:
  - Shortcuts you took and why
  - Things you're unsure about
  - Assumptions you made
  - Edge cases you considered but didn't handle
```

### Observing evidence decisions

Run with `RALPH_DIAGNOSTICS=1` to see evidence validation in real time. The diagnostics output includes:
- Which evidence checks passed or failed
- Why a `build.done` was rewritten to `build.blocked`
- Mutation score warnings
- Quality threshold failures with the specific dimensions that didn't meet the bar

### Thrashing detection

If the same task gets `build.blocked` 3 times consecutively, Ralph marks it as abandoned and eventually terminates the loop. This prevents infinite retry loops when an agent can't figure out how to pass backpressure. Custom topics don't benefit from this safety net.

---

## Summary: Who Controls What

| Actor | What they control |
|-------|-------------------|
| **Ralph's engine** | The three gated topics, the hard-coded thresholds (80% coverage, 70% mutation, 10.0 complexity), event transformation logic |
| **Your `ralph.yml`** | The mutation warning threshold (advisory only), which preset to use, hat topology |
| **Your hat instructions** | Evidence payload format guidance, confession thresholds, what "quality" means for your domain |
| **The agent** | Actual test/lint/coverage results, the ConfessionReport content, confidence scores |
| **The event bus** | Topic-based routing — connects hat outputs to hat inputs, no opinion on content |
