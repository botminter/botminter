# Objective

Advance all GitHub issues for the assigned project that are ready for action. Work items are tracked as GitHub issues on the team repository, tagged with the project label to indicate which project they belong to.

## Work Scope

Handle all phases of the issue lifecycle:
- Triage and backlog management
- Epic design, planning, and story breakdown
- Story test design and implementation
- Code review and quality verification
- Infrastructure setup
- Documentation

## Completion Condition

Done when no actionable issues remain for the assigned project. An issue is actionable when:
- It belongs to the assigned project (identified by `project/<project-name>` label)
- Its current status indicates work that can be performed now
- It is not waiting on human review or approval
- It is not waiting on another team member

Skip issues that are blocked or waiting on human gates.

## Work Location

GitHub issues on the team repository, filtered by the assigned project's label.
