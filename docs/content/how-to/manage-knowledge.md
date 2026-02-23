# Manage Knowledge

This guide covers adding, organizing, and scoping knowledge and invariant files across the recursive hierarchy.

## Add team-level knowledge

Team knowledge applies to all members. Place files in the team repo's `knowledge/` directory:

```bash
cd ~/workspace/my-team
cat > knowledge/api-conventions.md << 'EOF'
# API Conventions

All REST endpoints follow the Kubernetes API conventions:
- Use `<resource>/<subresource>` naming
- Return proper status codes
- Include resource version for optimistic concurrency
EOF

git add knowledge/api-conventions.md
git commit -m "docs: add API conventions knowledge"
```

After pushing, agents pick up new knowledge on their next `.botminter/` pull.

## Add project-level knowledge

Project knowledge applies to all members working on a specific project:

```bash
cat > projects/my-project/knowledge/upgrade-flow.md << 'EOF'
# HyperShift Upgrade Flow

Control plane upgrades proceed in this order:
1. HostedCluster version bump
2. Control plane operator reconciliation
3. NodePool rolling update
EOF

git add projects/my-project/knowledge/upgrade-flow.md
git commit -m "docs: add HyperShift upgrade flow knowledge"
```

## Add member-level knowledge

Member knowledge applies only to a specific role:

```bash
cat > team/architect/knowledge/design-template.md << 'EOF'
# Design Doc Template

Every design must include:
1. Problem statement
2. Proposed solution
3. Alternatives considered
4. Security considerations
5. Acceptance criteria (Given-When-Then)
EOF

git add team/architect/knowledge/design-template.md
git commit -m "docs: add architect design template"
```

## Add invariants

Invariants are mandatory constraints. Add them at the appropriate scope:

=== "Team invariant"

    ```bash
    cat > invariants/code-review-required.md << 'EOF'
    # Code Review Required

    Every PR must have at least one approving review
    before the PO merges it.
    EOF
    ```

=== "Project invariant"

    ```bash
    cat > projects/my-project/invariants/no-breaking-changes.md << 'EOF'
    # No Breaking API Changes

    Changes must not break the NodePool reconciliation
    API contract without explicit approval.
    EOF
    ```

=== "Member invariant"

    ```bash
    cat > team/human-assistant/invariants/always-confirm.md << 'EOF'
    # Always Confirm

    Present all state-modifying decisions to the human
    via human.interact and wait for confirmation.
    EOF
    ```

## Browse knowledge via the CLI

Schema v2 teams can use `bm knowledge` commands to browse and manage knowledge files without navigating the directory structure manually:

```bash
bm knowledge list                    # List all knowledge and invariant files
bm knowledge list --scope member     # Filter by scope (team, project, member, member-project)
bm knowledge show knowledge/commit-convention.md  # Display a file's contents
bm knowledge                         # Launch an interactive Claude Code session with the knowledge-manager skill
```

The interactive mode (`bm knowledge` with no subcommand) spawns a Claude Code session pre-loaded with the knowledge-manager skill, enabling conversational knowledge management.

## Scoping rules

Knowledge and invariants follow the same recursive scoping. All levels are additive — more specific levels extend, not replace, more general ones:

| Level | Knowledge path | Invariant path |
|-------|---------------|----------------|
| Team | `knowledge/` | `invariants/` |
| Project | `projects/<project>/knowledge/` | `projects/<project>/invariants/` |
| Member | `team/<member>/knowledge/` | `team/<member>/invariants/` |
| Member+project | `team/<member>/projects/<project>/knowledge/` | — |
| Hat | `team/<member>/hats/<hat>/knowledge/` | — |

## Knowledge file guidelines

!!! tip "Writing effective knowledge files"
    - **One topic per file** — keep files focused and named descriptively
    - **Use markdown** — all knowledge files are markdown (`.md`)
    - **Write for agents** — be explicit and unambiguous; agents read these literally
    - **Include examples** — concrete examples help agents apply knowledge correctly
    - **Declare scope** — state which roles or situations the knowledge applies to

## Propagation

Knowledge and invariant changes propagate automatically:

1. Commit and push changes to the team repo
2. Agents pull `.botminter/` at the start of every board scan cycle
3. New knowledge is available on the next cycle

No restart required — knowledge and invariants are read from `.botminter/` paths each time the agent consults them.

## Related topics

- [Knowledge & Invariants](../concepts/knowledge-invariants.md) — recursive scoping model
- [Workspace Model](../concepts/workspace-model.md) — how `.botminter/` works
- [Design Principles](../reference/design-principles.md) — rules for knowledge and backpressure configuration
- [CLI Reference — Knowledge](../reference/cli.md#knowledge-management) — `bm knowledge list/show` command details
