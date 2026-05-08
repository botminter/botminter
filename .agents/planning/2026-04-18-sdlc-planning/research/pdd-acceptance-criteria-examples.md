# PDD Acceptance Criteria in BotMinter — Real Examples

Source: `projects/botminter/specs/`, `projects/botminter/.agents/planning/`, `projects/botminter/.planning/plans/`

## Four PDD Projects Found

1. **Console Web UI** — `.agents/planning/2026-03-22-console-web-ui/` (full lifecycle)
2. **Loop Inbox** — `.planning/plans/loop-inbox/` (full lifecycle with FR/NFR format)
3. **GitHub App Identity** — `specs/github-app-identity/` (full lifecycle with 3 sprints, 27 design-level AC + sprint-level AC)
4. **Team Design Skills** — `specs/team-design-skills/` (7 code-tasks, each with AC)

## Three Styles of Acceptance Criteria

### Style 1: Single-Line GWT (Design Docs, Sprint Prompts)

From `specs/github-app-identity/design.md` — 27 criteria, organized by subsystem:

```
1. Given `bm start superman`, when the team is resolved, then the command
   delegates to `team.start("superman")` — never directly to a formation.
7. Given `bm hire <role> --name superman`, when the operator completes the
   manifest flow, then a GitHub App named `{team}-superman` is created,
   credentials stored in keyring, App installed on team repo + project repos.
12. Given a member with stored App credentials, when the daemon starts,
    then an installation token is generated and delivered via refresh_token().
24. Given `bm fire superman`, then App uninstalled, credentials removed,
    member dir removed, manual deletion instructions printed.
```

### Style 2: Multi-Line GWT with Titles (Code Tasks)

From `specs/team-design-skills/tasks/task-01.code-task.md`:

```
1. **Convention is documented**
   - Given a freshly extracted team repo
   - When I look at `knowledge/team-agreements.md`
   - Then I find the full convention documented

4. **Profile extraction includes agreements**
   - Given `bm init` extracts a profile
   - When the team repo is created
   - Then the `agreements/` directory and knowledge file are present
```

### Style 3: GWT Embedded in FR Blocks (Requirements Docs)

From `.planning/plans/loop-inbox/requirements.md`:

```
### FR-1: Brain can send messages to a running loop
**Acceptance criteria:**
- Given a running loop, when the brain sends a message, then the message
  is persisted and available for the coding agent to read
- Given multiple messages sent before consumption, when the coding agent
  reads, then all messages are returned in chronological order
- Given no running loop, the message is still persisted (fire-and-forget)
```

## Key Observations

1. **All criteria are concrete and observable.** No vague language. Specific inputs, conditions, verifiable outcomes.
2. **Many map directly to automated tests.** Design docs include traceability sections mapping AC to code components.
3. **Regression criteria explicitly tagged.** Sprint prompts mark `(Regression) Given just test, when run, then all tests pass.`
4. **Known limitations declared inline.** When a criterion can't be fully verified, the limitation is stated within the AC.
5. **Negative cases included.** "Given macOS, then formation::create('local') returns 'not yet supported'."
6. **Consistent vocabulary.** Given/When/Then always bolded. MUST/SHOULD/MUST NOT (RFC 2119).

## Assessment: Can These Serve as Verification Criteria?

**Yes.** The acceptance criteria are already:
- Testable (concrete inputs and outputs)
- Observable (user-perspective outcomes, not implementation details)
- Structured (GWT format, numbered, organized by subsystem)
- Traceable (mapped to requirements and components)

They are functionally equivalent to GSD's `must_haves.truths` — observable behaviors from the user's perspective that define "done." The format is different (GWT prose vs YAML truths), but the content is the same kind of information.
