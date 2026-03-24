---
name: profile-browser
description: >-
  Browses and describes BotMinter profiles — team methodology templates that
  define roles, statuses, coding agents, and process conventions. Use when the
  operator asks to "list profiles", "show profiles", "what profiles are available",
  "describe a profile", "what roles does X have", or "compare profiles".
  Reads ~/.config/botminter/profiles/.
metadata:
  author: botminter
  version: 1.0.0
  category: profiles
  tags: [profiles, roles, statuses, methodology]
---

# Profile Browser

Browses and describes BotMinter profiles. Profiles are team methodology templates shipped with the `bm` binary, defining roles, process conventions, status workflows, and member skeletons.

## Data Sources

| Source | Path | Contains |
|--------|------|----------|
| Profiles directory | `~/.config/botminter/profiles/` | All installed profiles |
| Profile manifest | `~/.config/botminter/profiles/<name>/botminter.yml` | Roles, statuses, labels, coding agents |
| Process definition | `~/.config/botminter/profiles/<name>/PROCESS.md` | Workflow process documentation |
| Role definitions | `~/.config/botminter/profiles/<name>/roles/` | Per-role member skeletons |
| Knowledge | `~/.config/botminter/profiles/<name>/knowledge/` | Profile-level developer guidance |

## How to List Profiles

```bash
ls ~/.config/botminter/profiles/
```

If the directory does not exist or is empty, profiles need initialization:

> Profiles not initialized. Run `bm profiles init` to extract the built-in profiles.

## How to Describe a Profile

Read the profile manifest:

```bash
cat ~/.config/botminter/profiles/<name>/botminter.yml
```

Extract and present:
- **Name and description** — what this methodology is about
- **Roles** — available roles with descriptions
- **Statuses** — workflow states for issue tracking
- **Coding agents** — supported coding agents (e.g., Claude Code)
- **Labels** — issue classification labels

## How to Show Roles

From the manifest, list all roles:

```yaml
roles:
  - name: architect
    description: "Designs system architecture..."
  - name: developer
    description: "Implements features..."
```

For more detail on a specific role, read the role's member skeleton:

```bash
ls ~/.config/botminter/profiles/<name>/roles/<role>/
```

This shows what files a member hired into this role receives (ralph.yml, PROMPT.md, AGENTS.md, hats, skills, knowledge).

## How to Show Status Workflow

From the manifest, list all statuses:

```yaml
statuses:
  - name: "po:triage"
    description: "Initial triage by product owner"
  - name: "arch:design"
    description: "Architecture design phase"
```

For the full workflow documentation, read `PROCESS.md`:

```bash
cat ~/.config/botminter/profiles/<name>/PROCESS.md
```

## How to Compare Profiles

When the operator asks to compare profiles, read the manifest for each and present a side-by-side comparison:

| Aspect | Profile A | Profile B |
|--------|-----------|-----------|
| Roles | 3 (architect, developer, ...) | 1 (superman) |
| Statuses | 12 | 6 |
| Process | Full scrum with human gates | Compact solo workflow |

## Output Format

```
## Profile: <name>

**<display_name>** — <description>

### Roles
| Role | Description |
|------|-------------|
| architect | Designs system architecture |
| developer | Implements features |

### Status Workflow
| Status | Description |
|--------|-------------|
| po:triage | Initial triage |
| arch:design | Architecture phase |

### Coding Agents
- **claude-code** (default) — Claude Code by Anthropic
```

## CLI Quick Reference

| Task | Command |
|------|---------|
| List profiles | `bm profiles list` |
| Describe a profile | `bm profiles describe <name>` |
| Initialize profiles | `bm profiles init` |
| List roles in a profile | `bm roles list -t <team>` |
