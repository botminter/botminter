---
created: 2026-03-05T04:16:59.397Z
title: Improve coding agents display in profiles describe
area: cli
files:
  - crates/bm/src/commands/profiles.rs
---

## Problem

The "Coding Agents" section in `bm profiles describe` output is functional but could be more polished:

```
Coding Agents (1):
  claude-code (default)     Claude Code — context: AGENTS.md, dir: .claude, binary: claude
```

Internal details like `binary: claude` are exposed to end users who don't need them. The formatting is dense and could benefit from better grouping or progressive disclosure (e.g., hide binary/dir details unless `--verbose` is passed).

## Solution

Consider:
- Hide internal fields (binary, dir, context file) from default output
- Show them only with `--verbose` or a dedicated flag
- Better visual formatting (table or grouped display)
- Focus default output on what matters to users: agent name, whether it's default, and display name
