# Your First Journey

You've completed the [Bootstrap Your Team](bootstrap-your-team.md) guide — your team is set up, workspaces are provisioned, and the board is ready. This page walks you through your first end-to-end experience: creating an epic, launching your agent, watching it work, and reviewing its output.

We'll use the `scrum-compact` profile with a single `superman` agent working on a project called `my-project`.

## Verify your setup

Before you begin, confirm everything is in place.

### On disk

Your workzone should look like this:

```
workzone/
  my-team/
    team/                            # Team repo (control plane)
      team/superman-01/              # Member config
      projects/my-project/           # Project-specific dirs
    superman-01/                     # Member directory
      my-project/                    # Project fork clone (agent CWD)
        .botminter/                  # Team repo clone
        PROMPT.md → .botminter/...
        CLAUDE.md → .botminter/...
        ralph.yml
        src/                         # Your project's source code
          main.py
          config.py
        tests/
          test_main.py
        README.md
        pyproject.toml
```

Run a quick check:

```bash
bm status
bm members list
bm projects list
```

### On GitHub

Open your team repo on GitHub. You should see:

- **Labels** — `kind/epic`, `kind/story`, `kind/docs`, and `project/my-project` (created by `bm projects add`)
- **Project board** — linked to the repo, with a Status field containing all the pipeline statuses (`po:triage`, `arch:design`, etc.)
- **Views** — if you followed the `bm projects sync` instructions, you should have role-based views (PO, Architect, Developer, QE, Lead, Specialist). In the compact profile, all six views show the same agent's work partitioned by hat.

If any of these are missing, re-run `bm projects sync` to sync statuses and get the view setup instructions.

## Create your first epic

Go to your team repo on GitHub and create a new issue:

1. Click **New issue**
2. **Title**: Something concrete, e.g., `Add health check endpoint`
3. **Body**: Describe what you want. Be as specific or as high-level as you like — the agent's architect hat will produce a design doc from this. For example:

    ```markdown
    Add a `/healthz` endpoint that returns the application's health status.

    - Should check database connectivity
    - Should check external service availability
    - Return 200 with JSON body when healthy, 503 when unhealthy
    ```

4. **Labels**: Apply both:
    - `kind/epic` — marks this as an epic-level work item
    - `project/my-project` — associates it with your project

5. **Status**: On the Project board, set the Status field to `po:triage`

Your epic is now on the board, waiting for the agent to pick it up.

## Launch the agent

Start your team:

```bash
bm start
```

The agent launches as a Claude Code instance in its workspace. It runs on a loop — scanning the board, picking up work, processing it, and rescanning. There may be a short delay before the first scan cycle picks up your issue.

You can watch the agent's progress:

```bash
bm status -v
```

## The triage gate (your first interaction)

The agent's first action is to pick up your epic at `po:triage`. The PO hat reads the epic, evaluates it, and posts a **triage request comment** on the issue:

```markdown
### 📝 po — 2026-02-27T14:00:00Z

**Triage request**

New epic in triage: "Add health check endpoint"

Summary: [agent's evaluation of the epic]

Please respond on this issue:
- `Approved` — accept into backlog
- `Rejected: <reason>` — close this epic
```

!!! warning "You need to respond"
    The agent does NOT auto-approve. The epic stays at `po:triage` until you respond. On each scan cycle, the agent checks for your response — if none is found, it moves on and checks again next cycle.

**To approve:** Add an issue comment:

```
@bot Approved
```

**To reject:** Add an issue comment with your feedback:

```
@bot Rejected: The scope is too broad. Let's focus on the /healthz endpoint only, without external service checks.
```

Once you approve, the agent moves the epic to `po:backlog`.

Alternatively, you can move the issue's Status to `po:backlog` directly on the Project board — the agent will pick it up on the next scan.

!!! note "Why `@bot`?"
    Since all agents share a single GitHub token, the agent and the human post comments as the same GitHub user. Prefixing your comments with `@bot` lets the agent distinguish your responses from its own comments. This is a temporary convention — per-role GitHub tokens are planned, which will eliminate the need for the prefix.

## Backlog activation

At `po:backlog`, the agent posts a backlog report comment. The epic parks here until you're ready to start work on it.

**To activate:** Add an issue comment:

```
@bot start
```

Or move the issue's Status to `arch:design` directly on the Project board.

The agent picks this up on the next scan and begins the design phase.

## Watch the pipeline

From here, the agent drives the epic through the pipeline — switching hats at each stage. Here's what to expect:

### Design phase

1. **`arch:design`** — The architect hat reads the epic and project codebase, produces a design document (stored in the team repo under `projects/my-project/knowledge/designs/`), and posts a summary comment.

2. **`lead:design-review`** — The lead reviewer hat reviews the architect's design. This is an automated quality gate — the lead hat checks the design for completeness and either approves (advancing to human review) or rejects (sending back to the architect with feedback).

3. **`po:design-review`** — Your review gate. The PO reviewer hat posts a review request comment:

    ```markdown
    ### 📝 po — 2026-02-27T15:00:00Z

    **Design review request**

    Epic: "Add health check endpoint"

    [Summary of the design]

    Please respond on this issue:
    - `Approved` — proceed to planning
    - `Rejected: <feedback>` — revise the design
    ```

    **To approve:**

    ```
    @bot Approved
    ```

    **To reject:**

    ```
    @bot Rejected: The design doesn't address error handling for when the database is unreachable.
    ```

    If you reject, the agent reverts to `arch:design` and incorporates your feedback. You can also move the Status directly on the Project board to approve or reject.

### Planning phase

4. **`arch:plan`** — The architect hat breaks the approved design into stories with acceptance criteria and posts the story breakdown as a comment.

5. **`lead:plan-review`** — The lead reviewer hat reviews the story breakdown.

6. **`po:plan-review`** — Your review gate. Same approve/reject flow as the design review (`@bot Approved` or `@bot Rejected: <feedback>`), or move the Status on the board.

### Breakdown and execution

7. **`arch:breakdown`** — The architect creates individual story issues on the team repo, each labeled `kind/story` and linked to the parent epic via a `parent/<number>` label.

8. **`lead:breakdown-review`** — The lead reviewer hat reviews the created story issues.

9. **`po:ready`** — The epic parks here. Stories are created, and the epic waits for you to activate execution.

    **To activate:** Comment on the epic issue:

    ```
    @bot start
    ```

    Or move the Status to `arch:in-progress` directly on the Project board.

10. **`arch:in-progress`** — The architect hat monitors story execution. Each story goes through its own pipeline:

    ```
    qe:test-design → dev:implement → dev:code-review → qe:verify → arch:sign-off → po:merge → done
    ```

    - **`qe:test-design`** — The QE hat designs tests and writes test stubs *before* implementation (test-first approach)
    - **`dev:implement`** — The developer hat implements the story against the test stubs
    - **`dev:code-review`** — The code review hat reviews the implementation. Can reject back to `dev:implement` with feedback.
    - **`qe:verify`** — The QE hat verifies the implementation against acceptance criteria. Can reject back to `dev:implement` with feedback.
    - **`arch:sign-off`** and **`po:merge`** — Auto-advance gates handled by the board scanner. No manual action needed.

### Final acceptance

11. **`po:accept`** — When all stories are complete, the epic reaches your final review gate. Review the completed work and comment to close or revise:

    ```
    @bot Approved
    ```

    Or to send it back:

    ```
    @bot Rejected: The health check doesn't return the expected JSON schema. See the acceptance criteria.
    ```

12. **`done`** — Epic complete.

## Summary of human interaction points

Throughout the epic lifecycle, you'll interact at these gates:

| Gate | Status | Comment | Or move Status to |
|------|--------|---------|-------------------|
| Triage | `po:triage` | `@bot Approved` or `@bot Rejected: <reason>` | `po:backlog` |
| Backlog activation | `po:backlog` | `@bot start` | `arch:design` |
| Design review | `po:design-review` | `@bot Approved` or `@bot Rejected: <feedback>` | `arch:plan` or `arch:design` |
| Plan review | `po:plan-review` | `@bot Approved` or `@bot Rejected: <feedback>` | `arch:breakdown` or `arch:plan` |
| Ready activation | `po:ready` | `@bot start` | `arch:in-progress` |
| Final acceptance | `po:accept` | `@bot Approved` or `@bot Rejected: <feedback>` | `done` or `arch:in-progress` |

The agent never auto-approves at any of these gates. You can interact via comments or by moving the Status directly on the Project board.

!!! warning "Shared token limitation"
    Currently, all agents share a single GitHub token — so the agent and you post comments as the same GitHub user. The `@bot` prefix on your comments helps the agent distinguish your responses from its own. Per-role GitHub tokens are planned for a future release, which will remove the need for this prefix.

## Monitor progress

While the agent works, you can check in anytime:

```bash
# Agent status
bm status -v

# See the issue activity on GitHub
gh issue list -R <your-org>/team-repo --state open
```

On the GitHub Project board, switch between views (PO, Architect, Developer, etc.) to see where issues sit in the pipeline.

## What to do when things go wrong

### Nothing happens after creating the epic

Make sure the epic has:
- The `kind/epic` label
- The `project/<name>` label
- Its Status field set to `po:triage` on the Project board

Then check that the agent is running with `bm status -v`.

### The epic is stuck at a review gate

If an issue sits at `po:triage`, `po:design-review`, `po:plan-review`, `po:accept`, `po:backlog`, or `po:ready`, the agent is waiting for your comment. Check the issue for a review request comment and respond with `@bot Approved`, `@bot Rejected: <feedback>`, `@bot start`, or `@bot activate` as appropriate. Don't forget the `@bot` prefix. See the [human interaction table](#summary-of-human-interaction-points) above.

### Agent gets stuck on an issue

If an issue reaches `error` status, the agent has failed to process it 3 times. Check the issue comments for error details, fix the underlying problem, then manually reset the Status field on the GitHub Project board to the appropriate pipeline stage.

### Agent picks up the wrong issue

Labels matter. Make sure every issue has exactly one `kind/*` label and the correct `project/<name>` label.

## Stop the agent

When you're done:

```bash
bm stop
```

Or to force-stop immediately:

```bash
bm stop --force
```

## Next steps

- Read [The Agentic Workflow](../workflow.md) to understand the philosophy behind the process
- Learn about [Knowledge & Invariants](../concepts/knowledge-invariants.md) to customize what your agent knows
- Explore the [Coordination Model](../concepts/coordination-model.md) for details on board scanning and handoffs
- See [Profiles](../concepts/profiles.md) to understand how to switch to a multi-member team
