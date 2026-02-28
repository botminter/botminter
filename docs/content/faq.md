# FAQ

## Does BotMinter only work with Claude Code?

Claude Code is the primary supported runtime today. Under the hood, BotMinter uses a pluggable runtime layer — currently powered by [Ralph orchestrator](https://github.com/mikeyobrien/ralph-orchestrator). We're working on making it easy to plug in custom runtimes as well.

## Do I need multiple agents running in parallel?

Not at all. It depends on the profile you choose. The `scrum-compact` profile runs a single agent that wears multiple hats — planner, implementer, reviewer — all in one. Other profiles like `scrum` distribute those hats across specialized agents. You pick the formation that fits your workflow.

## Is this only for Scrum teams?

No. BotMinter focuses on **conventions**, not a specific methodology. It ships with opinionated defaults (like the Scrum profile), but profiles are fully customizable. Think of it like Rails for web or Spring for enterprise — baked-in conventions that you can tweak to fit your process. You can fork an existing profile or create one from scratch.

## What's the difference between BotMinter and just using Claude Code?

Claude Code is a powerful single-agent tool. BotMinter adds structured conventions on top: layered knowledge scoping (team → project → member), process enforcement via status gates, cumulative learning through knowledge files, and a coordination fabric via GitHub issues. The value isn't in the runtime — it's in the conventions that make your investment in one agent carry forward to every other agent you run.

## What is a "profile"?

A profile is a reusable methodology template — it defines roles, pipeline stages, quality gates, and knowledge structure. `bm init` extracts a profile into a new team repo. You can use the built-in profiles as-is, fork them, or create your own.

## Why do I need to prefix my comments with `@bot`?

Currently, all agents share a single GitHub token — so the agent and you post comments as the same GitHub user. The `@bot` prefix helps the agent distinguish your responses from its own comments. This is a temporary convention. Per-role GitHub tokens are planned, which will let each agent (or hat) use its own GitHub identity and eliminate the need for the prefix.
