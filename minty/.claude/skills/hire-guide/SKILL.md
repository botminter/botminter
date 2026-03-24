---
name: hire-guide
description: >-
  Interactive guide for hiring team members with `bm hire`. Helps operators
  choose roles, pick names, and understand hiring implications. Use when the
  operator asks to "hire someone", "add a member", "what roles can I hire",
  "help me hire", "who should I hire next", or "explain the roles".
metadata:
  author: botminter
  version: 1.0.0
  category: team-management
  tags: [hiring, roles, members, onboarding]
---

# Hire Guide

Interactive guide for hiring BotMinter team members. Walks the operator through role selection, name choice, and explains what happens when a member is hired.

## How Hiring Works

When the operator runs `bm hire <role> --name <name> -t <team>`:

1. The role is validated against the team's profile
2. A member directory is created at `<team-path>/team/members/<name>/`
3. The role's member skeleton is copied (ralph.yml, PROMPT.md, AGENTS.md, hats, skills, knowledge)
4. The member is registered in the team configuration
5. After hiring, `bm teams sync` provisions the workspace

## Guiding the Operator

### Step 1: Identify Available Roles

Read the team's profile manifest to list available roles:

```bash
# If the operator has a team
cat <team-path>/team/botminter.yml
```

Or use the CLI:

```bash
bm roles list -t <team>
```

If no team exists yet, browse profiles to show what roles are available:

```bash
bm profiles list
bm profiles describe <profile>
```

### Step 2: Explain Each Role

For each available role, explain:
- **What it does** — the role's responsibilities from the profile description
- **What it gets** — hat collection, skills, knowledge files
- **When you need it** — scenarios where this role adds value

Read the role skeleton for details:

```bash
ls ~/.config/botminter/profiles/<profile>/roles/<role>/
cat ~/.config/botminter/profiles/<profile>/roles/<role>/ralph.yml
```

### Step 3: Help Choose a Name

Member names must be:
- Lowercase alphanumeric with hyphens
- Unique within the team
- Descriptive (e.g., `lead-architect`, `frontend-dev`, `docs-writer`)

Suggest names based on the role and team composition:
- First member of a role: use the role name (e.g., `architect`)
- Multiple members in same role: add a qualifier (e.g., `backend-dev`, `frontend-dev`)
- Specialized focus: reflect the specialization (e.g., `api-architect`, `test-engineer`)

### Step 4: Confirm and Execute

Show the operator the exact command:

```bash
bm hire <role> --name <name> -t <team>
```

After hiring, remind them to sync:

```bash
bm teams sync -t <team>
```

## Handling Edge Cases

### No Teams Configured

If `~/.botminter/config.yml` does not exist:

> No teams registered yet. Before hiring, create a team with `bm init`.

### Role Already Filled

Multiple members can have the same role. Explain that each gets an independent workspace and Ralph Orchestrator instance.

### Unknown Role

If the operator asks about a role not in the profile:

> That role isn't available in the `<profile>` profile. Available roles are: <list>.
> Roles are defined by the profile. To add custom roles, modify the profile.

## CLI Quick Reference

| Task | Command |
|------|---------|
| List available roles | `bm roles list -t <team>` |
| Hire a member | `bm hire <role> --name <name> -t <team>` |
| List current members | `bm members list -t <team>` |
| Show member details | `bm members show <member> -t <team>` |
| Sync after hiring | `bm teams sync -t <team>` |
