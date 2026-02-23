# Ralph Orchestrator: RObot & Human-in-the-Loop Interaction

> How Ralph enables bidirectional communication between agents and humans during orchestration — blocking questions, non-blocking notifications, proactive guidance, and timeout behavior.

## TL;DR

**Ralph has two distinct channels for agent-to-human communication, and one channel for human-to-agent.** Agents can send blocking questions (the loop pauses) or fire-and-forget notifications (the loop continues). Humans can reply to questions or send unsolicited guidance at any time. Currently, **there is no way to make the loop halt permanently if a question goes unanswered** — on timeout, the loop always continues.

---

## The Three Communication Channels

Ralph's human-in-the-loop system runs over Telegram and operates through three event types, each with fundamentally different loop behavior.

### 1. Blocking Questions — `human.interact`

The agent emits a `human.interact` event when it needs a decision. The orchestration loop **blocks** — no new iterations run until a response arrives or the timeout expires.

```bash
ralph emit "human.interact" "Decision needed: should we use SQLite or PostgreSQL? Options: (A) SQLite — simpler, no infra. (B) PostgreSQL — scales better. Default if no response: SQLite"
```

What happens mechanically:

| Step | What Happens |
|------|-------------|
| 1 | Agent calls `ralph emit "human.interact" "..."` during its iteration |
| 2 | Orchestrator reads the event from JSONL, detects the `human.interact` topic |
| 3 | Sends the question to Telegram (with retry: 3 attempts, exponential backoff at 1s, 2s, 4s) |
| 4 | Loop **blocks** — polls the events file every 250ms for a `human.response` event |
| 5a | Human replies → `human.response` event is written → loop unblocks, response injected into next iteration |
| 5b | Timeout expires → loop unblocks, continues **without** a response |
| 5c | Send fails after all retries → treated as timeout, loop continues without blocking |

**The question includes context.** When the bot sends the question to Telegram, it wraps it with the current hat name, iteration number, and loop ID — so the human knows what the agent is working on.

**Cooldown is skipped.** When a `human.response` arrives, the orchestrator skips the normal `cooldown_delay_seconds` wait before the next iteration. You don't want the agent sitting idle after the human answered.

### 2. Non-Blocking Notifications — `ralph tools interact progress`

The agent sends a one-way message. The loop does **not** block — the agent continues working immediately.

```bash
ralph tools interact progress "All tests passing — starting integration phase"
```

This is a **direct Telegram API call**, not an event on the bus. No event is written to JSONL, no hat routing occurs, nothing blocks. It's fire-and-forget.

Use for:
- Phase transitions ("moving from implementation to testing")
- Milestone completions ("feature X is done, starting feature Y")
- FYI updates ("tests are slow, this might take a while")
- Status updates between long iterations

### 3. Proactive Human Guidance — `human.guidance`

The human sends a message to the Telegram bot at any time — not as a reply to a question, just unsolicited input. This becomes a `human.guidance` event.

What happens:

| Step | What Happens |
|------|-------------|
| 1 | Human sends a message in the Telegram chat (not replying to a question) |
| 2 | The bot's message handler writes a `human.guidance` event to `events.jsonl` |
| 3 | On the **next iteration**, the orchestrator reads the event |
| 4 | Guidance is collected, squashed into a numbered list, and injected as a `## ROBOT GUIDANCE` section in the agent's prompt |
| 5 | Guidance is also persisted to the scratchpad with a timestamp, so it survives process restarts |

Multiple guidance messages arriving between iterations are squashed:

```
## ROBOT GUIDANCE

1. Focus on error handling first
2. Use the existing retry pattern from the auth module
3. Don't bother with the legacy endpoint — it's being removed
```

A single guidance message is injected as-is (no numbering).

**Guidance placement in the prompt:** `OBJECTIVE` → `ROBOT GUIDANCE` → `PENDING EVENTS`. The agent sees guidance before it sees what work to do, so guidance can steer how the agent approaches the pending events.

**Guidance is persistent but single-use in the prompt.** It's injected into one iteration's prompt, then cleared from memory. But because it's also written to the scratchpad, the agent can still reference it in later iterations if needed — it just won't appear as a dedicated `## ROBOT GUIDANCE` section again.

---

## How the Agent Learns About These Channels

The agent doesn't discover `ralph tools interact progress` or `ralph emit "human.interact"` from your hat instructions. It learns from a **built-in skill** called `robot-interaction` that Ralph automatically injects into every prompt when `RObot.enabled: true`.

### The injection mechanism

Ralph has a dedicated injection path for the robot skill — it's not loaded on-demand like other skills. When `RObot.enabled` is `true`, the event loop unconditionally prepends the full skill content into the prompt prefix, wrapped in `<robot-skill>` tags. The agent sees this every iteration:

```
<robot-skill>
# Human Interaction (RObot)

A human is available via Telegram. You can ask blocking questions
or send non-blocking progress updates.

## Progress updates (non-blocking)

Send one-way notifications — the loop does NOT block:

    ralph tools interact progress "All tests passing — starting integration phase"

Use for: phase transitions, milestone completions, status updates, FYI messages.

## Asking questions (blocking)

Emit human.interact with your question — the loop blocks until the human replies or timeout:

    ralph emit "human.interact" "Decision needed: [1 sentence]. Options: (A) ... (B) ...
    Default if no response: [what you'll do]"

Always include:
1. The specific decision (1 sentence)
2. 2-3 concrete options with trade-offs
3. What you'll do if no response (timeout fallback)
</robot-skill>
```

This skill also includes decision guidance — when to use blocking vs. non-blocking, when to use neither, and the hard rule about never sending both in the same turn.

### What this means for your hat instructions

**You don't need to teach the agent the commands.** The skill handles that. Your hat instructions only need to express *when* and *why* to communicate — not *how*. For example:

```yaml
hats:
  builder:
    name: "Builder"
    triggers: ["build.task"]
    publishes: ["build.done", "human.interact"]
    instructions: |
      Send a progress update when you finish each phase.
      Ask for human approval before any database migration.
```

The agent already knows that "send a progress update" means `ralph tools interact progress` and "ask for human approval" means `ralph emit "human.interact"` — because the robot skill told it so.

### When `RObot.enabled` is `false`

The skill is not injected at all. The agent has no knowledge of Telegram, progress notifications, or `human.interact`. If a hat's instructions mention asking the human, the agent won't know how to do it — there's no fallback mechanism.

### Difference from other skills

Most skills use a two-tier model: a compact index line is always in the prompt, and the agent loads full content on-demand via `ralph tools skill load <name>`. The `robot-interaction` skill skips this — its full content is always present. This is because the agent needs to know about human interaction from the very first iteration, not after it discovers a skill index and decides to load it.

The `ralph-tools` skill (for tasks and memories) works the same way — always injected when its feature is enabled, never on-demand. These two built-in skills are the only ones with this unconditional injection behavior.

---

## Timeout Behavior in Detail

### What happens on timeout

When a `human.interact` question goes unanswered for `timeout_seconds`:

1. The pending question is removed from the Telegram state
2. `wait_for_response` returns `None`
3. The orchestrator logs `"Human response timeout — continuing without response"`
4. The loop proceeds to the next iteration **as if the question was never asked**

The agent does **not** receive a "timed out" event or notification. It simply doesn't get a `human.response` in its next iteration's context. From the agent's perspective, the question disappeared.

### Can you make the agent never act without an answer?

**Not with configuration alone.** There is no `block_until_response: true` or `timeout_seconds: infinity` option. The timeout is always finite, and when it fires, the loop always continues.

Your options within the current system:

#### Option A: Very large timeout

```yaml
RObot:
  enabled: true
  timeout_seconds: 86400  # 24 hours
  telegram:
    bot_token: "your-token"
```

This buys you up to 24 hours to respond. The loop will genuinely block for that entire duration, polling every 250ms. Not a true "block forever" but effectively infinite for most interactive workflows.

**Trade-off:** If you walk away and forget, the loop sits idle consuming a process slot for 24 hours. No iteration budget is consumed during the block (the iteration counter only advances when the agent actually runs), but the process is alive and holding the loop lock.

#### Option B: Instruct the agent via hat instructions

You can't change what the orchestrator does on timeout, but you can change what the **agent** does when it doesn't get an answer. Add explicit instructions to your hat:

```yaml
hats:
  careful_builder:
    name: "Careful Builder"
    triggers: ["build.task"]
    publishes: ["build.done", "human.interact"]
    instructions: |
      ## CRITICAL: Human Approval Required

      Before making any irreversible change (database migration, public API change,
      deleting files), you MUST ask for human approval via:

        ralph emit "human.interact" "Approval needed: [describe what you're about to do]"

      If your question was not answered (no human.response in your context), do NOT
      proceed with the change. Instead:
      1. Log what you were going to do in the scratchpad
      2. Emit human.interact again with the same question
      3. Do NOT work on anything else until you get approval

      This is a hard rule. Never assume silence means approval.
```

**Trade-off:** This relies on the LLM following instructions, which is probabilistic, not deterministic. Unlike backpressure gates which mechanically reject events, this is a prompt-level contract. The agent *could* ignore it — especially under context pressure or in edge cases. But in practice, strongly-worded hat instructions are followed consistently.

#### Option C: Combine both

Use a large timeout (Option A) so the orchestrator blocks for a long time, **and** instruct the agent (Option B) so that even if the timeout fires, the agent re-asks rather than proceeding.

```yaml
RObot:
  enabled: true
  timeout_seconds: 3600  # 1 hour
  telegram:
    bot_token: "your-token"

hats:
  gated_builder:
    triggers: ["build.task"]
    publishes: ["build.done", "human.interact"]
    instructions: |
      For any destructive or irreversible action, ask for approval via human.interact.
      If you don't receive a human.response, re-ask. Never assume silence means consent.
```

This gives you a 1-hour mechanical block followed by a behavioral re-ask loop. The agent would keep re-emitting `human.interact` each iteration until it gets an answer (and each re-ask triggers another 1-hour block).

#### What Option C looks like in practice

```
Iteration 5:  Agent emits human.interact → loop blocks for up to 1 hour
              ... no answer ...
              Timeout fires → loop continues
Iteration 6:  Agent sees no human.response → re-emits human.interact → blocks again
              ... human answers 20 minutes in ...
              human.response received → loop unblocks
Iteration 7:  Agent sees the response, proceeds with the approved action
```

Each re-ask costs one iteration (the agent runs, decides it still needs approval, re-emits). With `max_iterations` set high enough, this creates an effective "block until answered" loop — burning one iteration per timeout cycle, but never proceeding without approval.

---

## Parallel Loops and Message Routing

When multiple loops are running (primary + worktree loops), messages need to reach the right loop.

### Routing rules

| Message Type | How It's Routed |
|-------------|-----------------|
| Reply to a bot question | Routes to the loop that asked the question (tracked by message ID in Telegram state) |
| Message starting with `@loop-id` | Routes to the specified loop (e.g., `@fix-auth Use JWT not sessions`) |
| Any other message | Routes to the **primary loop** (the one holding `.ralph/loop.lock`) |

### Important: only the primary loop runs the bot

The Telegram bot starts only on the primary loop. Worktree loops don't run their own bot instance. This means:

- The primary loop's bot handles all incoming messages and routes them
- Worktree loops communicate through the shared events file
- If the primary loop stops, Telegram communication stops for all loops

---

## The `progress` vs. `human.interact` Decision

The agent's skill instructions include a hard rule: **never send `progress` and `human.interact` in the same turn.** If you need to ask a question, fold the status update into the question. This avoids confusing the human with a notification immediately followed by a blocking question.

### When the agent should use each

| Scenario | Channel | Why |
|----------|---------|-----|
| Phase transition | `progress` | FYI — no decision needed |
| Routine status update | `progress` | No blocking required |
| Ambiguous requirements | `human.interact` | Can't proceed without clarification |
| Irreversible action | `human.interact` | Need explicit approval |
| Conflicting signals | `human.interact` | Need a tiebreaker |
| Scope question | `human.interact` | "Should I also do X?" |
| Anything answerable from specs, code, or context | **Neither** | Don't bother the human |
| Rephrasing a question already asked | **Neither** | Don't re-ask the same thing differently |

---

## Error Handling and Edge Cases

### Send failures

When Ralph can't deliver a question to Telegram:

1. Retries 3 times with exponential backoff (1s delay, then 2s, then 4s)
2. If all retries fail, the failure is logged to diagnostics
3. The loop **continues without blocking** — treated identically to a timeout

The agent gets no indication that its question wasn't delivered. From its perspective, it's the same as "asked but nobody answered."

### Bot not configured but `human.interact` emitted

If an agent emits `human.interact` but no RObot service is active (RObot disabled or misconfigured), the event is logged at debug level and **passed through to the bus as a regular event**. It won't block the loop and won't reach any human. Ralph handles it as the universal fallback subscriber.

### Process restart during a block

If the Ralph process is killed while blocking on a `human.interact`:

- The pending question is left in `.ralph/telegram-state.json`
- On restart, the state file is loaded but the block is **not resumed** — the question is effectively lost
- The human may still reply in Telegram, but the response won't be matched to a pending question

### Graceful shutdown during a block

If Ralph receives SIGTERM or SIGINT (Ctrl+C) while blocking:

- The shutdown flag is detected during the poll loop
- The pending question is cleaned up from state
- `wait_for_response` returns `None`
- The loop exits cleanly

---

## Configuration Reference

```yaml
RObot:
  enabled: true                    # Required. Default: false
  timeout_seconds: 300             # Required when enabled. How long to block on human.interact
  telegram:
    bot_token: "your-token"        # Required. Or use RALPH_TELEGRAM_BOT_TOKEN env var
    api_url: "https://api.telegram.org"  # Optional. For mock servers in CI/testing
```

| Field | Required | Default | Notes |
|-------|----------|---------|-------|
| `enabled` | Yes | `false` | Must be `true` for any HLI features |
| `timeout_seconds` | When enabled | — | Validation fails without it. No upper bound enforced. |
| `telegram.bot_token` | When enabled | — | Env var `RALPH_TELEGRAM_BOT_TOKEN` takes precedence over config |
| `telegram.api_url` | No | Telegram's default | Env var `RALPH_TELEGRAM_API_URL` takes precedence |

### Environment variables (override config values)

| Variable | Overrides |
|----------|-----------|
| `RALPH_TELEGRAM_BOT_TOKEN` | `RObot.telegram.bot_token` |
| `RALPH_TELEGRAM_API_URL` | `RObot.telegram.api_url` |

---

## Bot Commands

While a loop is running, you can interact with the bot beyond just answering questions:

| Command | What It Does |
|---------|-------------|
| `/status` | Current loop status |
| `/tasks` | Open tasks |
| `/memories` | Recent memories |
| `/tail` | Last 20 events |
| `/restart` | Restart the loop |
| `/stop` | Stop the loop at the next iteration boundary |
| `/help` | List available commands |

These are handled by the bot directly — they don't emit events on the bus.

---

## Summary: Who Controls What

| Actor | What They Control |
|-------|-------------------|
| **Ralph's engine** | The blocking/unblocking mechanics, timeout enforcement, event routing, cooldown skip for human events |
| **Your `ralph.yml`** | Whether RObot is enabled, timeout duration, bot token, API URL |
| **Your hat instructions** | When the agent asks questions, what it does on timeout, whether it re-asks |
| **The agent** | Choosing between `progress` and `human.interact`, question phrasing, timeout fallback behavior |
| **The human** | Answering questions, sending proactive guidance, bot commands |
| **Telegram** | Message delivery, chat ID detection, reply threading for multi-loop routing |
