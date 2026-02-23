//! End-to-end tests for the `bm` CLI.
//!
//! These tests exercise `bm` against real external services (GitHub, Telegram
//! mock). They are feature-gated behind `--features e2e` and run serially
//! via `just e2e` (`--test-threads=1`).
//!
//! Prerequisites:
//! - `gh auth status` must succeed (GitHub tests)
//! - `podman` must be available (Telegram mock tests)

mod helpers;

mod daemon_lifecycle;
mod github;
mod init_to_sync;
mod start_to_stop;
mod telegram;

/// Skip the test if `gh auth status` fails.
macro_rules! require_gh_auth {
    () => {
        if !github::gh_auth_ok() {
            eprintln!("SKIP: gh auth not available — skipping test");
            return;
        }
    };
}

// ── Smoke test ───────────────────────────────────────────────────────

/// Proves the E2E harness works: creates a temp GitHub repo, spins up
/// tg-mock, exercises the control API, and tears both down via RAII.
#[test]
fn e2e_harness_smoke() {
    require_gh_auth!();

    // ── Part 1: GitHub harness ───────────────────────────────────────
    match github::TempRepo::new("bm-e2e-smoke") {
        Ok(repo) => {
            // Verify we can query the repo
            let labels = github::list_labels(&repo.full_name);
            eprintln!(
                "GitHub smoke: repo {} has {} default labels",
                repo.full_name,
                labels.len()
            );

            let issues = github::list_issues(&repo.full_name);
            assert!(
                issues.is_empty(),
                "fresh repo should have no issues, found: {:?}",
                issues
            );
            // Drop deletes the repo
        }
        Err(e) => {
            eprintln!(
                "SKIP: GitHub repo creation failed (likely token lacks \
                 CreateRepository permission): {}",
                e
            );
        }
    }

    // ── Part 2: Telegram mock harness ────────────────────────────────
    if telegram::podman_available() {
        let mock = telegram::TgMock::start();
        let token = "test-token-smoke";
        let chat_id = 12345i64;

        // Inject a fake user message
        mock.inject_message(token, "hello from e2e smoke test", chat_id);

        // Query the request log (no bot is running, so no bot requests)
        let requests = mock.get_requests(token, "sendMessage");
        eprintln!(
            "Telegram smoke: tg-mock has {} sendMessage requests",
            requests.len()
        );

        // Drop stops and removes the container
    } else {
        eprintln!("SKIP: podman not available — skipping tg-mock smoke");
    }
}
