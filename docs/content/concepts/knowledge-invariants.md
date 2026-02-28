# Knowledge & Invariants

BotMinter uses a three-tier model for guiding and constraining agent behavior: **knowledge**, **invariants**, and **backpressure**. All three follow a recursive scoping model where more specific levels extend (never replace) more general ones.

## Knowledge

Knowledge files contain domain context, conventions, and reference material that agents consult when doing work. Knowledge is **lazy** — hat instructions list the directories, and the agent decides what is relevant for the current task.

Examples of knowledge files:

- Commit conventions (`knowledge/commit-convention.md`)
- PR review standards (`knowledge/pr-standards.md`)
- Communication protocols (`knowledge/communication-protocols.md`)
- Project-specific architecture docs

### Knowledge resolution order

Knowledge resolves from most general to most specific. All levels are additive:

| Level | Path | Scope |
|-------|------|-------|
| Team | `.botminter/knowledge/` | Applies to all members |
| Project | `.botminter/projects/<project>/knowledge/` | Project-specific |
| Member | `.botminter/team/<member>/knowledge/` | Role-specific |
| Member+project | `.botminter/team/<member>/projects/<project>/knowledge/` | Role + project specific |
| Hat | `.botminter/team/<member>/hats/<hat>/knowledge/` | Hat-specific |

More specific knowledge takes precedence when there are conflicts, but all applicable knowledge is available to the agent.

## Invariants

While knowledge provides advisory context, invariants enforce **mandatory constraints**. All applicable invariants must be satisfied — they are additive across scopes.

### Invariant scoping

| Level | Path | Example |
|-------|------|---------|
| Team | `.botminter/invariants/` | Code review required, test coverage |
| Project | `.botminter/projects/<project>/invariants/` | Project-specific quality rules |
| Member | `.botminter/team/<member>/invariants/` | Role-specific constraints |

Declare invariants in the member's `CLAUDE.md` under an `# INVARIANTS` section. Claude Code injects `CLAUDE.md` natively into every hat, so invariants apply universally within a member.

## Backpressure

Beyond mandatory constraints, individual hats use backpressure gates to enforce quality at transition points.

Backpressure gates are per-hat quality checks that must pass before a hat can transition an issue's status. They are defined in each hat's `### Backpressure` section in `ralph.yml`.

Backpressure differs from invariants:

| Aspect | Invariants | Backpressure |
|--------|-----------|--------------|
| Scope | All hats via CLAUDE.md | Per hat |
| Granularity | General rules (team/project/member) | Specific verifiable conditions |
| Purpose | Universal constraints | Gate status transitions |
| Configuration | File-based (`.md` files) | Inline in hat instructions |

Backpressure gates define **what** success looks like, not **how** to achieve it.

???+ example "Example: backpressure gate (scrum designer hat)"
    > Before transitioning to `status/po:design-review`, verify:
    >
    > - Design doc has a Security Considerations section
    > - Design doc has acceptance criteria (Given-When-Then)
    > - Design doc references applicable project knowledge

## Guardrails

Where backpressure applies per hat, guardrails apply universally. Ralph injects `core.guardrails` from `ralph.yml` as `### GUARDRAILS` (numbered 999+) into every hat prompt.

Use guardrails for cross-cutting behavioral constraints that apply regardless of which hat is active:

- Lock discipline rules
- Invariant compliance requirements
- Universal safety rules

## Summary

```
Behavior Control Model:
├── Knowledge (advisory context)
│   └── Lazy — agent consults as needed
│   └── Scoped: team → project → member → hat
├── Invariants (mandatory constraints)
│   └── Enforced — all applicable invariants must be satisfied
│   └── Scoped: team → project → member
├── Backpressure (per-hat quality gates)
│   └── Gate — blocks status transitions until conditions met
│   └── Scoped: per hat in ralph.yml
└── Guardrails (universal rules)
    └── Injected — into every hat prompt
    └── Scoped: all hats via core.guardrails
```

## Related topics

- [Architecture](architecture.md) — three-layer model where knowledge and invariants live
- [Profiles](profiles.md) — how profiles package knowledge and invariants
- [Manage Knowledge](../how-to/manage-knowledge.md) — adding and organizing knowledge files
- [Design Principles](../reference/design-principles.md) — rules for configuring knowledge, invariants, and backpressure
