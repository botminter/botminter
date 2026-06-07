# Run Meetings

This guide covers running meetings with your team members and defining custom meetings in a profile.

## Prerequisites

- A team initialized with a profile that defines meetings (e.g., `agentic-sdlc-planning`)
- At least one member hired for the role referenced by the meeting
- Members hired (`bm hire <role>`)

## Running a meeting

List available meetings with `--help`:

```bash
bm meetings --help
```

Start a meeting by name:

```bash
bm meetings planning
```

This resolves the meeting's `member` field to a hired member of that role, writes the meeting's `instructions` as the system prompt, and launches the coding agent with the meeting's `prompt` as the initial message.

### Passing context

Append free-form text after the meeting name to pass additional context:

```bash
bm meetings planning plan the auth feature for project-x
```

If the meeting defines a `prompt` (e.g., `start`), your input is appended to it. If there is no `prompt`, your input becomes the initial message.

| Meeting has `prompt` | You pass args | Agent receives |
|---------------------|---------------|----------------|
| `start` | `plan the auth feature` | `start plan the auth feature` |
| `start` | *(none)* | `start` |
| *(none)* | `plan the auth feature` | `plan the auth feature` |
| *(none)* | *(none)* | *(no initial message — you type first)* |

!!! warning "Flags must come before free-form text"
    Place `-t` and `-a` **before** the free-form input. Flags after the first word of input are captured as part of the text sent to the agent, not parsed as CLI flags.

    ```bash
    # Correct — flag before input
    bm meetings planning -t my-team plan the auth feature

    # Wrong — flag is sent to the agent as text
    bm meetings planning plan the auth feature -t my-team
    ```

### Targeting a specific team

```bash
bm meetings planning -t my-team
```

### Running with skip-permissions

Use `-a` to pass `--dangerously-skip-permissions` to the coding agent:

```bash
bm meetings planning -a
```

## Defining meetings in a profile

Meetings are defined in the `meetings` list in a profile's `botminter.yml`. Each meeting becomes a subcommand of `bm meetings`.

### Minimal meeting

```yaml
meetings:
  - name: standup
    description: "Daily standup check-in"
    member: engineer
    instructions: |
      You are an engineer on this team.
      The operator wants a quick standup update.
      Review the GitHub Project board and summarize:
      - What was completed since the last standup
      - What is in progress
      - Any blockers
```

This creates `bm meetings standup`. No `prompt` field — the operator types the first message.

### Meeting with initial prompt

```yaml
meetings:
  - name: verification
    description: "Verify acceptance criteria for completed work"
    member: engineer
    instructions: |
      You are an engineer on this team.
      You are in a verification meeting with the human (PO).
      You MUST load the `verification` skill to verify acceptance criteria.
      Ask the user for the work item (issue number) to verify.
    prompt: start
```

Adding `prompt: start` means the agent receives `start` as the first message and begins immediately — no waiting for the operator to type.

### Meeting with skill loading

Meetings don't inherit skills from the member's hat configuration. If the meeting needs a skill, tell the agent to load it in the instructions:

```yaml
meetings:
  - name: retro
    description: "Sprint retrospective"
    member: chief-of-staff
    instructions: |
      You are the chief of staff on this team.
      You are running a sprint retrospective.
      You MUST load the `retrospective` skill.
      Guide the operator through:
      1. What went well
      2. What didn't go well
      3. Action items for next sprint
    prompt: start
```

### Multiline instructions

Use YAML literal block scalars (`|`) for multiline instructions:

```yaml
instructions: |
  First line of the system prompt.
  Second line.

  Blank lines are preserved.
  - Markdown formatting works
  - Lists, headers, code blocks — all valid
```

## Testing a meeting definition

After editing meetings in your team repo's `botminter.yml`:

1. Verify the meeting appears:

    ```bash
    bm meetings --help
    ```

2. Run it:

    ```bash
    bm meetings <name>
    ```

To test changes to the profile source (for future teams), edit `~/.config/botminter/profiles/<profile>/botminter.yml`, then create a new team with `bm init` to see the updated meetings.

!!! note "Profile vs. team repo"
    Meetings are defined in the profile source (`~/.config/botminter/profiles/<profile>/botminter.yml`) and copied into the team repo during `bm init`. At runtime, `bm meetings` reads from the **team repo's** `botminter.yml`, not the profile on disk. To update meetings for an existing team, edit `<team_path>/team/botminter.yml` directly. To update them for future teams, edit the profile source and re-init.

## Troubleshooting

**"No meetings defined"**
: The active profile doesn't have a `meetings` section in `botminter.yml`. Add one or switch to a profile that includes meetings (e.g., `agentic-sdlc-planning`).

**"Member not found for role"**
: The meeting references a role (e.g., `engineer`) but no member of that role is hired. Run `bm hire <role>` first.

**"No workspace found"**
: Ensure members are hired with `bm hire <role>`. Session workspaces are created automatically when meetings start.

## Related topics

- [Meetings Concepts](../concepts/meetings.md) — what meetings are and how they differ from chat
- [Profiles](../concepts/profiles.md) — profile structure and customization
- [CLI Reference — `bm meetings`](../reference/cli.md#bm-meetings) — full command documentation
