# Exploratory Test Scope

Exploratory tests MUST only exercise user-facing CLI commands, never internal or hidden commands.

## Rule

Exploratory tests MUST only invoke commands that an operator would use in production:

- `bm init`, `bm hire`, `bm start`, `bm stop`, `bm status`, `bm members list`, `bm members show`, `bm teams show`, `bm projects add`
- Bridge-level operations: `bm bridge start`, `bm bridge stop`, `bm bridge status`, `bm bridge identity add <member>`, `bm bridge identity list`, `bm bridge room create <name>`
- Session management: `bm session list`, `bm session inspect`, `bm session cleanup`, `bm session finalize`
- Infrastructure tools the operator interacts with (`curl` against bridge APIs, `gh` for GitHub verification, `podman` for container inspection)

Note: `bm teams sync` and `bm teams sync --bridge` were removed from the CLI. Exploratory tests MUST NOT reference these removed commands.

Exploratory tests MUST NOT directly invoke internal or hidden commands (`bm brain-run`, `bm daemon-tick`, or any command not documented in `bm --help` top-level output). Testing internal commands belongs in unit tests and integration tests.

Exploratory tests MUST NOT bypass the user path by directly manipulating internal state (e.g., writing to `.ralph/events-*.jsonl` files instead of sending messages through the bridge). If the user path is "send a Matrix message," the test must send a Matrix message — not write a file that the system would normally produce.

## Applies To

- All test cases in `crates/bm/tests/exploratory/` (PLAN.md and Justfile)
- Any new exploratory test phases added to the suite

Does **NOT** apply to:

- Unit tests (`cargo test -p bm`) — these test internals by design
- Integration tests (`crates/bm/tests/`) — these test module boundaries
- E2E tests (`crates/bm/tests/e2e/`) — governed by `e2e-scenario-coverage.md`
- Conformance tests — these validate bridge spec compliance

## Examples

**Compliant:**
```
# Test brain lifecycle through user-facing commands
bm start                    # operator starts members
bm status                   # operator checks status
curl Matrix API             # operator verifies bridge messaging
bm stop                     # operator stops members
```

**Compliant:**
```
# Validate brain autonomy by sending a message through the bridge
# (the way a human user would interact with the member)
curl -X PUT Matrix send API   # human sends message to room
sleep + poll room history     # wait for brain member to respond
assert response in room       # verify autonomous response
```

**Violating:**
```
# Testing internal command directly
bm brain-run --help
bm brain-run --workspace /path --system-prompt /path
```
Internal commands are not part of the operator journey.

**Violating:**
```
# Bypassing bridge by writing event files directly
echo '{"topic":"human.interact",...}' > .ralph/events-test.jsonl
```
In production, events arrive via the bridge (Matrix messages), not via file writes. The test must use the real delivery path.

**Violating:**
```
# Running cargo test and counting passes inside exploratory suite
cargo test -p bm brain::queue
assert $PASS_COUNT -ge 8
```
Unit test verification is not an exploratory test. It tests developer artifacts, not user experience.

## Rationale

Phase H exploratory tests included `bm brain-run` tests (H15-H17) and direct event file injection (H48-H50). A reviewer approved these because they were technically correct — the tests passed, the code compiled, the Justfile was well-structured. It required human feedback to identify that these tests violated the purpose of exploratory testing: simulating what a real user does. This invariant codifies that boundary so reviewers (human and agent) can catch scope violations mechanically, without needing domain intuition about what "exploratory" means.
