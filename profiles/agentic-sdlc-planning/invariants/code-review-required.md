# Code Review Required

## Rule
All code changes require review before merge. In the agentic SDLC minimal profile, review is performed by the `dev_code_reviewer` hat using `gh pr review` (approve/request-changes). Code review happens on the PR, not via issue comments.

## Applies To
All code-producing hats (dev_implementer, architect). Applies to all project repo contributions.

## Verification
Every code change passes through the `dev_code_reviewer` hat at `eng:dev:code-review` before proceeding to QE verification at `eng:qe:verify`. The PR review record confirms the review occurred. No code reaches `eng:qe:verify` without an approved PR review.

---
*Placeholder — to be populated with detailed review requirements before the team goes live.*
