# Manage Members

This guide covers hiring team members from profile skeletons and understanding the member directory structure.

## Hire a member

Use the `bm hire` command:

```bash
bm hire <role>
```

Example:

```bash
bm hire human-assistant
```

This extracts the member skeleton from the embedded profile into `team/<role>/`, including all configuration files, and creates a Git commit.

You can optionally provide a name and target a specific team:

```bash
bm hire architect --name bob -t my-team
```

If you omit `--name`, `bm` auto-generates a 2-digit suffix (e.g., `architect-01`). Auto-suffix fills gaps: if `01` and `03` exist, it returns `02`.

## Available roles

The available roles depend on the profile. Use `bm roles list` to see them:

```bash
bm roles list
```

For the `scrum` profile:

| Role | Description |
|------|-------------|
| `human-assistant` | PO's proxy — board scanning, backlog management, review gating |
| `architect` | Technical authority — design docs, story breakdowns, issue creation |

If you specify a role that doesn't exist, the command lists all available roles.

## Member directory structure

After hiring a member, the `team/<role>/` directory contains the following structure. The specific files (invariants, hats, etc.) depend on the profile and role. This example shows the `human-assistant` role from the `scrum` profile:

```
team/human-assistant/
├── PROMPT.md              # Role identity and behavioral rules
├── CLAUDE.md              # Role context (workspace model, knowledge paths)
├── ralph.yml              # Ralph orchestrator configuration
├── .botminter.yml         # Member metadata (role name, emoji)
├── knowledge/             # Role-specific knowledge
├── invariants/            # Role-specific constraints
│   └── always-confirm.md  # (example: present all decisions to human)
├── hats/                  # Hat-specific directories (if applicable)
│   └── <hat-name>/
│       └── knowledge/     # Hat-specific knowledge
├── agent/
│   ├── skills/            # Role-specific skills
│   └── agents/            # Role-specific sub-agents
└── projects/              # Per-project role config
    └── <project>/
        └── knowledge/     # Role+project-specific knowledge
```

## Customize a member

After hiring a member, you can customize its configuration:

### Modify prompts

Edit `team/<role>/PROMPT.md` to change role identity and behavioral rules. Edit `team/<role>/CLAUDE.md` to update role context.

!!! note
    In workspaces, `PROMPT.md` and `CLAUDE.md` are symlinks into `.botminter/`. Changes to the source files in the team repo propagate automatically when agents pull `.botminter/`.

### Add role-specific knowledge

Place knowledge files in `team/<role>/knowledge/`:

```bash
echo "# Debug Guide\n\nAlways check pod logs first." \
  > team/architect/knowledge/debug-guide.md
git add team/architect/knowledge/debug-guide.md
git commit -m "docs: add architect debug guide"
```

### Add role-specific invariants

Place invariant files in `team/<role>/invariants/`:

```bash
echo "# Design Quality\n\nEvery design must include security considerations." \
  > team/architect/invariants/design-quality.md
git add team/architect/invariants/design-quality.md
git commit -m "chore: add architect design quality invariant"
```

### Modify Ralph configuration

Edit `team/<role>/ralph.yml` to change hat definitions, event routing, or persistence settings.

!!! warning
    `ralph.yml` is copied (not symlinked) to workspaces. After editing it, run `bm teams sync` and restart the agent.

## Remove a member

Delete the member directory and commit:

```bash
rm -rf team/human-assistant
git add -A
git commit -m "chore: remove human-assistant member"
```

!!! warning
    This does not remove existing workspaces. Delete workspace directories separately.

## List members

View all hired members for a team:

```bash
bm members list
```

## Related topics

- [Launch Members](launch-members.md) — provisioning workspaces and starting agents
- [Member Roles](../reference/member-roles.md) — detailed role definitions and hat models
- [Knowledge & Invariants](../concepts/knowledge-invariants.md) — scoping model
