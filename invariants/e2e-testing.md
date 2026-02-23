# Invariant: E2E Testing for External API Interactions

## Rule

Any code that constructs payloads for external APIs (GitHub GraphQL, REST, `gh` CLI commands) **MUST** have a corresponding E2E test that executes the real API call and verifies the result.

Unit tests and integration tests with mocked/absent credentials are necessary but **not sufficient** for this class of code. Serialization bugs, escaping issues, and payload format errors are invisible to tests that don't hit the real service.

## What qualifies as an external API interaction

- GraphQL mutations constructed as strings (e.g., `updateProjectV2Field`)
- `gh` CLI commands that create, modify, or query GitHub resources
- Any `Command::new("gh")` call that produces side effects

## E2E test requirements

1. **Location:** `crates/bm/tests/e2e/` — feature-gated behind `--features e2e`
2. **Auth gate:** Use `require_gh_auth!()` macro so tests skip gracefully without credentials
3. **RAII cleanup:** Use `TempRepo`, `TempProject`, or similar guards that clean up on drop (even on panic)
4. **Isolation:** Use `tempfile::tempdir()` for HOME — never touch `~/.botminter`
5. **Verify the remote state:** Don't just check exit code and stdout. Query the API to confirm the mutation actually took effect (e.g., `list_project_status_options` after sync)
6. **Test idempotency:** Re-run the command and verify it succeeds again

## Rationale

This invariant exists because a GraphQL escaping bug (`\\\"` vs `\"`) shipped past 12 unit tests, 4 integration tests, and clippy — and was only caught by a user running the command manually. An E2E test that hits the real GitHub API catches this class of bug in under 15 seconds.

## Constants for E2E tests

- **Org:** `devguyio-bot-squad`
- **Persistent test repo:** `devguyio-bot-squad/test-team-repo`
- **Auth:** `GH_TOKEN` env var (same as production)
- **Run command:** `cargo test -p bm --features e2e --test e2e -- --test-threads=1`
