# Ralph Orchestrator: Event Loop & Hat Chaining

> How Ralph handles event-driven hat transitions — and why there's no "direct fire" between hats.

## TL;DR

**There is no tight event chaining.** When Hat A publishes an event that Hat B subscribes to, Ralph does NOT fire Hat B directly. Every hat transition goes through the full orchestrator loop. Ralph (the "Hatless Ralph" coordinator) is always the executor — custom hats only define the event topology.

---

## How the Loop Works

Each iteration of a Ralph run follows this sequence:

1. **Check termination** — max iterations, max runtime, max cost, stop signal, etc.
2. **Pick what to do** — Ralph checks which hats have pending events
3. **Build the prompt** — Ralph collects all pending events and determines the "active hat" for this iteration
4. **Run the agent** — one LLM invocation with the active hat's instructions baked in
5. **Read emitted events** — picks up anything the agent wrote via `ralph emit`
6. **Check for completion** — looks for `LOOP_COMPLETE`
7. **Cooldown** — waits `cooldown_delay_seconds` before the next iteration
8. **Repeat**

There is no shortcut. Every hat activation requires a full trip through this cycle.

## Ralph Is Always the Executor

In multi-hat mode, **Ralph runs every iteration**. Custom hats never execute independently — they're topology declarations. Ralph reads the hat config to know:

- Which hat's instructions to follow this iteration (based on pending events)
- What events the hat is allowed to publish
- Where to route the work next

Think of it like a single actor wearing different "hats" each iteration, not a team of actors passing work between each other.

## Example: A Two-Hat Pipeline

Given this config:

```yaml
hats:
  implementer:
    name: "Implementer"
    triggers: ["task.*"]
    publishes: ["work.done"]
  reviewer:
    name: "Reviewer"
    triggers: ["work.done"]
    publishes: ["review.done"]
```

Here's what actually happens when the implementer finishes:

| Iteration | What Ralph Does |
|-----------|----------------|
| N | Runs as **implementer**. Agent calls `ralph emit work.done "implemented feature X"` |
| *(between iterations)* | Ralph reads the emitted event, sees `work.done`, routes it to **reviewer**'s queue |
| N+1 | Runs as **reviewer**. Agent sees `work.done` payload and reviews the work |

There's always a full loop iteration — including the cooldown delay — between hat transitions.

## The `starting_event` and Fast Path

### Default behavior (no `starting_event`)

The loop starts by publishing `task.start`. Ralph receives it, plans, and delegates to the first hat.

### With `starting_event` configured

```yaml
event_loop:
  starting_event: "tdd.start"
```

The loop publishes `tdd.start` immediately. The hat subscribed to `tdd.start` gets it in its queue. **But Ralph still runs first** — there is no way to skip the initial Ralph iteration.

### Fast path (fresh run only)

On a fresh run (no existing scratchpad), Ralph detects the `starting_event` and activates a **fast path**: it immediately re-emits the starting event via `ralph emit` without planning. This costs one iteration but skips the usual plan-then-delegate cycle.

On resume (`ralph resume`) or when the scratchpad already exists, the fast path is disabled. Ralph plans and delegates normally.

**Bottom line:** Even in the best case, `starting_event` costs one "pass-through" iteration before the target hat begins real work.

## Event Routing Rules

When an event is emitted via `ralph emit`:

- **Specific subscriptions take priority.** If `reviewer` subscribes to `work.done` specifically, it gets the event — Ralph's catch-all `*` subscription does not compete.
- **Self-routing is allowed.** If a hat emits an event it also subscribes to, the event routes back to itself. This handles cases where the LLM emits an event that triggers more work for the same hat.
- **Human events (`human.*`) bypass normal routing** and go through a separate queue.

## Backpressure: Hardcoded Quality Gates

Ralph validates certain event topics before accepting them. **These are exact-match, hardcoded topic names** — not patterns or configurable rules. If validation fails, the event is **rewritten** to a blocked/failed variant that routes back to the originating hat.

### The gated topics

| Exact Topic String | What Ralph Checks | If Validation Fails |
|--------------------|-------------------|---------------------|
| `build.done` | Must include evidence: tests, lint, typecheck, audit, coverage, complexity, duplication | Becomes `build.blocked` — hat retries |
| `review.done` | Must include `tests: pass` and `build: pass` (literal substrings) | Becomes `review.blocked` — hat retries |
| `verify.passed` | Must include quality report: `quality.tests`, `quality.lint`, `quality.audit` (pass), `quality.coverage` (>= 80%), `quality.mutation` (>= 70%), `quality.complexity` (<= 10) | Becomes `verify.failed` — hat retries |
| `verify.failed` | Warns if missing quality report, but **passes through** | *(unchanged)* |
| Completion event | Must be the last event in the batch | Silently ignored |
| **Any other topic** | **No validation — passes through unchanged** | *(unchanged)* |

### When gates apply (and when they don't)

The gates are **always active** — whether you have one hat or ten, fresh run or resume. There is no config flag to disable them.

However, the gates **only apply to events written via `ralph emit`** (i.e., events read from the JSONL file). Several internal paths publish events directly to the bus and bypass validation entirely:

| Event source | Goes through gates? |
|---|---|
| Agent calls `ralph emit build.done "..."` | **Yes** — validated |
| Agent calls `ralph emit review.done "..."` | **Yes** — validated |
| Agent calls `ralph emit verify.passed "..."` | **Yes** — validated |
| Agent calls `ralph emit some.custom.topic "..."` | **No** — passes through unchecked |
| Agent writes nothing → `default_publishes` fires | **No** — injected directly to bus, even if the default is `build.done` |
| Orchestrator injects fallback (`task.resume`) | **No** — direct bus publish |
| Orchestrator injects hat exhaustion event | **No** — direct bus publish |
| Loop initialization (`task.start` or `starting_event`) | **No** — direct bus publish |

The `default_publishes` bypass is notable: if a hat has `default_publishes: "build.done"` and the agent fails to emit any event, the orchestrator injects a `build.done` with an empty payload directly into the bus — **no evidence is checked**. Several built-in presets are configured this way.

### How the agent learns about evidence format

The agent receives evidence format instructions through **three layers**, with gaps in each:

**1. The `EVENT WRITING` section (always present in the prompt)**

Every prompt includes this example:

```
ralph emit "build.done" "tests: pass, lint: pass, typecheck: pass, audit: pass, coverage: pass"
ralph emit "review.done" --json '{"status": "approved", "issues": 0}'
```

The `build.done` example shows the correct format. However, **the `review.done` example is wrong** — the gate checks for the literal substrings `tests: pass` and `build: pass`, but the example shows a JSON payload with `status` and `issues`. An agent following this example would get `review.blocked`.

**2. Auto-generated instructions (only for hats without custom `instructions`)**

When a hat has no `instructions` field in its YAML, Ralph auto-generates guidance from the pub/sub contract. For example, a hat triggering on `build.task` gets told *"Run backpressure (tests/lint/typecheck/audit/coverage/specs)"* — but without specifying the payload format. For publishing `build.done`, it only gets *"When implementation is finished and tests pass."*

There are **no auto-generated instructions** for publishing `review.done` or `verify.passed` at all.

Most presets provide custom `instructions`, which means the agent never sees these auto-generated hints.

**3. The hat's own instructions (from YAML config)**

The hat's custom `instructions` field is where most presets put their quality guidance. But none of the built-in presets include the specific evidence payload format in their instructions. They say things like "run tests" and "publish event with evidence" without specifying *how* to format that evidence in the `ralph emit` payload.

### What this means for custom hats

If your hat publishes a custom topic like `feature.done` or `bugfix.done`, **it gets no backpressure validation**. The agent can claim "done" without any evidence and the orchestrator won't stop it.

To opt into quality enforcement, your hats must publish the literal string `build.done`. There is no way to apply `build.done` validation rules to a differently-named topic.

**Funneling through `build.done`** (recommended for quality enforcement):

```yaml
hats:
  feature_builder:
    name: "Feature Builder"
    triggers: ["task.feature"]
    publishes: ["build.done"]     # gets backpressure validation
  bugfixer:
    name: "Bug Fixer"
    triggers: ["task.bugfix"]
    publishes: ["build.done"]     # gets backpressure validation
  reviewer:
    name: "Reviewer"
    triggers: ["build.done"]
    publishes: ["review.done"]    # also gets validation
```

Both hats must include evidence in their `ralph emit build.done` payload (e.g., `tests: pass`, `lint: pass`, etc.). If they skip it, they get bounced back as `build.blocked`.

If you use `build.done` with custom hats, consider adding the evidence format explicitly in your hat's `instructions` — don't rely on the agent picking it up from the example in `EVENT WRITING`:

```yaml
instructions: |
  ## Builder
  ...after tests pass, publish with evidence:
  ralph emit "build.done" "tests: pass, lint: pass, typecheck: pass, audit: pass, coverage: pass, complexity: <score>, duplication: pass"
```

**Using custom topics** (no enforcement):

```yaml
hats:
  feature_builder:
    name: "Feature Builder"
    triggers: ["task.feature"]
    publishes: ["feature.done"]   # passes through unchecked
  bugfixer:
    name: "Bug Fixer"
    triggers: ["task.bugfix"]
    publishes: ["bugfix.done"]    # passes through unchecked
```

Total freedom, but the orchestrator can't prevent a hat from claiming success without proof.

### Non-code reviewers: use `verify.passed` with semantic mapping

The `review.done` gate requires the literal strings `tests: pass` and `build: pass` — unsuitable for documentation, design, or any non-code review hat.

The better option is `verify.passed`. Its quality dimensions are abstract enough to map to any review domain. The parser only checks for `quality.*:` key-value pairs and numeric thresholds — it doesn't care what the dimensions *mean*. You redefine the semantics in the hat's `instructions`.

Here's a docs reviewer example:

```yaml
hats:
  docs_reviewer:
    name: "Docs Reviewer"
    triggers: ["docs.ready"]
    publishes: ["verify.passed", "verify.failed"]
    instructions: |
      ## DOCS REVIEWER

      Review documentation for accuracy, clarity, and completeness.

      ### Quality Dimensions (mapped for docs)

      When publishing your review, map your findings to quality dimensions:

      - `quality.tests` = **Accuracy** — Do code examples work? Do CLI commands produce shown output? → pass/fail
      - `quality.lint` = **Formatting** — Consistent markdown, headings, code fences, style? → pass/fail
      - `quality.audit` = **Completeness** — All features documented? No gaps, dead links, missing sections? → pass/fail
      - `quality.coverage` = **Section coverage** — % of spec acceptance criteria covered by docs → number (gate requires >= 80)
      - `quality.mutation` = **Freshness** — % of docs that reflect current code, not stale behavior → number (gate requires >= 70)
      - `quality.complexity` = **Readability** — Jargon density, sentence complexity, assumed knowledge. Lower is better → number (gate requires <= 10)
      - `quality.specs` = **Matches spec** — Does the doc satisfy the original requirements? → pass/fail (optional, but fail blocks)

      ### How to publish

      If docs pass review:
        ralph emit "verify.passed" "quality.tests: pass, quality.lint: pass, quality.audit: pass, quality.coverage: 95, quality.mutation: 85, quality.complexity: 4"

      If docs fail review:
        ralph emit "verify.failed" "quality.tests: fail, quality.lint: pass, quality.audit: fail, quality.coverage: 60, quality.mutation: 40, quality.complexity: 8"
      Write a summary of what needs fixing to the scratchpad.
```

This gives you orchestrator-enforced quality gates for documentation review without needing any code changes.

**Thresholds to keep in mind** — these are hardcoded and can't be tuned per-hat:

| Dimension | Threshold | What it means for docs |
|-----------|-----------|----------------------|
| `quality.coverage` | >= 80% | At least 80% of spec criteria must be documented |
| `quality.mutation` | >= 70% | At most 30% of docs can be stale/outdated |
| `quality.complexity` | <= 10 | Works naturally as a 1-10 readability score |

If these floors are too lenient or too strict for your use case, you can nudge the agent in the instructions (e.g., "aim for 95% coverage") — but the gate itself only enforces the floor. The agent can't lower the bar, only raise it.

This same pattern works for any non-code reviewer: design reviews, API contract reviews, security policy reviews — map the `quality.*` dimensions to whatever makes sense for the domain.

### Thrashing detection only applies to `build.blocked`

If the same task gets `build.blocked` 3 times consecutively, Ralph marks it as abandoned and eventually terminates the loop. Custom topics don't benefit from this safety net — a hat publishing `feature.done` repeatedly without progress will never be auto-abandoned.

### Built-in presets and gate alignment

Most built-in presets sidestep the gates, sometimes by design and sometimes accidentally:

| Preset | Topic used | Hits gate? | Notes |
|--------|-----------|------------|-------|
| `feature.yml` builder | `build.done` | Yes | Hat instructions don't specify evidence format |
| `feature.yml` reviewer | `review.approved` | No | Different topic than `review.done` |
| `refactor.yml` verifier | `verify.passed` | Yes | Instructions don't specify quality report format |
| `docs.yml` reviewer | `review.done` | Yes | Gate expects tests/build evidence — wrong for docs |
| `deploy.yml` verifier | `verify.pass` | No | Note: `verify.pass` ≠ `verify.passed` |
| `fresh-eyes.yml` builder | `build.complete` | No | Note: `build.complete` ≠ `build.done` |
| `review.yml` reviewer | `review.complete` | No | Custom topic |
| `spec-driven.yml` verifier | `task.complete` | No | Custom topic |

The one-word differences (`build.complete` vs `build.done`, `verify.pass` vs `verify.passed`) are the difference between "free pass" and "blocked." Be precise with topic names.

## Fallback Recovery

If the agent forgets to emit any event (no pending events after an iteration), Ralph injects a `task.resume` fallback to keep the loop alive. This gives the agent another chance to figure out what to do. After 3 consecutive fallback attempts with no progress, the loop terminates.

## Practical Implications

| Consideration | What It Means for You |
|---------------|----------------------|
| **Iteration budget** | A 3-hat pipeline costs at minimum 4 iterations (1 Ralph + 3 hats). Add backpressure retries on top. Set `max_iterations` accordingly. |
| **Cooldown between hats** | Every hat transition includes `cooldown_delay_seconds`. Set to `0` if speed matters more than rate limiting. |
| **No parallelism within a loop** | Hats run sequentially through Ralph. For parallel work, use separate worktree loops (`ralph run` in multiple terminals). |
| **`starting_event` saves one planning step** | Useful when you know exactly which hat should start — skips Ralph's initial planning on fresh runs. |
| **Diagnostics show the real flow** | Run with `RALPH_DIAGNOSTICS=1` to see hat selection and event routing per iteration in `.ralph/diagnostics/`. |
| **Spell your topic names exactly** | `build.done` ≠ `build.complete`. One hits the gate, the other passes through. There's no fuzzy matching. |
| **Include evidence format in hat instructions** | Don't rely on the agent inferring the format from the `EVENT WRITING` example. Be explicit in your hat's `instructions` field. |
