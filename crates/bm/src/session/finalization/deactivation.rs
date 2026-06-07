use std::path::Path;

use anyhow::Result;

use super::categorize::{self, Category, RepoContext, RepoKind, TrackingStatus};
use super::subagent;
use crate::session::dirty_state::RepoDirtyState;
use crate::session::types::SessionId;

pub const DEFAULT_MAX_RETRIES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizationOutcome {
    Completed,
    CompletedDegraded,
    Failed(String),
    Skipped,
}

#[derive(Debug, Clone)]
pub struct FinalizationResult {
    pub outcome: FinalizationOutcome,
    pub recovery_branches: Vec<String>,
}

impl FinalizationResult {
    pub fn new(outcome: FinalizationOutcome) -> Self {
        Self {
            outcome,
            recovery_branches: Vec::new(),
        }
    }
}

/// Determines whether any repository in the dirty state warrants finalization.
///
/// Returns `true` if:
/// - Any repo has committed-but-unpushed branches, OR
/// - Any repo has uncommitted files that categorize as `CommitAndPush`.
///
/// Returns `false` if all repos are either clean or contain only credential
/// files or files on the default branch.
pub fn has_committable_files(workspace_path: &Path, dirty_state: &[RepoDirtyState]) -> bool {
    for repo in dirty_state {
        if !repo.unpushed_branches.is_empty() {
            return true;
        }

        let context = build_repo_context(workspace_path, repo);
        for file in &repo.uncommitted_files {
            let file_path = extract_porcelain_path(file);
            if categorize::categorize(Path::new(file_path), &context) == Category::CommitAndPush {
                return true;
            }
        }
    }
    false
}

/// Finalize a session by launching the finalization subagent if dirty state
/// contains files that need committing.
///
/// Returns `Skipped` if no finalization is needed, `Completed` if the
/// subagent was successfully launched (fire-and-forget), or `Failed` if
/// the subagent could not be spawned.
pub fn finalize_session(
    session_id: &SessionId,
    workspace_path: &Path,
    dirty_state: &[RepoDirtyState],
) -> FinalizationResult {
    let has_dirty = dirty_state.iter().any(|r| !r.is_clean());
    if !has_dirty {
        return FinalizationResult::new(FinalizationOutcome::Skipped);
    }

    if !has_committable_files(workspace_path, dirty_state) {
        return FinalizationResult::new(FinalizationOutcome::Skipped);
    }

    match subagent::retrigger_finalization(workspace_path, session_id) {
        Ok(_child) => FinalizationResult::new(FinalizationOutcome::Completed),
        Err(e) => FinalizationResult::new(FinalizationOutcome::Failed(
            format!("Failed to launch finalization subagent: {e}"),
        )),
    }
}

/// Re-trigger finalization for a retained session by launching the finalization subagent.
///
/// Returns the spawned child so callers can attach a watcher (e.g., `wait_and_transition`).
pub fn retrigger_finalization(
    session_id: &SessionId,
    workspace_path: &Path,
) -> Result<std::process::Child> {
    subagent::retrigger_finalization(workspace_path, session_id)
}

/// Directly push the currently checked-out branch for repos that have committed-but-unpushed
/// branches.
///
/// Fast path for the common "committed-but-not-yet-pushed" case (e.g. D22 test scenario).
/// Runs before spawning the LLM finalization agent — avoids Vertex AI latency when a plain
/// `git push` is sufficient. Returns `Ok(())` if every repo with unpushed branches was pushed
/// (or skipped because HEAD is detached); returns `Err` on the first push failure so callers
/// can fall through to the LLM agent.
///
/// Pushes the HEAD branch only (not `--all`) to avoid pushing branches from other sessions
/// that share the same bare clone, and because bare-clone worktrees have no `refs/remotes/`
/// so `--all` would attempt to push every branch in the shared object store.
pub fn push_unpushed_repos(workspace_path: &Path, dirty_state: &[RepoDirtyState]) -> Result<()> {
    for repo in dirty_state {
        if repo.unpushed_branches.is_empty() {
            continue;
        }
        let repo_path = if repo.repo_name == "team" {
            workspace_path.join("team")
        } else {
            workspace_path.join("projects").join(&repo.repo_name)
        };

        // Determine the current branch. A detached HEAD (e.g. the worktree was
        // provisioned with `--detach` and the agent never checked out a branch)
        // has nothing to push via this fast path.
        let head_output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&repo_path)
            .output()?;
        let branch = String::from_utf8_lossy(&head_output.stdout)
            .trim()
            .to_string();
        if branch.is_empty() || branch == "HEAD" {
            continue;
        }

        let output = std::process::Command::new("git")
            .args(["push", "origin", &branch])
            .current_dir(&repo_path)
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "git push failed for repo {} branch {}: {}",
                repo.repo_name,
                branch,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}

/// Push the current branch with fetch+rebase retry on non-fast-forward failures.
///
/// On each attempt: push → if NFF, fetch origin + rebase onto the remote tracking
/// branch → retry push.  Gives up after `max_retries` consecutive failures.
/// Returns `Ok(())` on success or the last push error on exhaustion.
pub fn push_with_rebase_retry(repo_path: &Path, max_retries: usize) -> Result<()> {
    let head_output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()?;
    let branch = String::from_utf8_lossy(&head_output.stdout)
        .trim()
        .to_string();
    if branch.is_empty() || branch == "HEAD" {
        return Ok(());
    }

    for attempt in 0..=max_retries {
        let push = std::process::Command::new("git")
            .args(["push", "origin", &branch])
            .current_dir(repo_path)
            .output()?;

        if push.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&push.stderr);
        let is_nff = stderr.contains("non-fast-forward")
            || stderr.contains("fetch first")
            || stderr.contains("rejected");

        if !is_nff || attempt == max_retries {
            return Err(anyhow::anyhow!(
                "git push failed for branch {}: {}",
                branch,
                stderr.trim()
            ));
        }

        let fetch = std::process::Command::new("git")
            .args(["fetch", "origin", &branch])
            .current_dir(repo_path)
            .output()?;
        if !fetch.status.success() {
            return Err(anyhow::anyhow!(
                "git fetch failed: {}",
                String::from_utf8_lossy(&fetch.stderr).trim()
            ));
        }

        let rebase = std::process::Command::new("git")
            .args(["rebase", &format!("origin/{branch}")])
            .current_dir(repo_path)
            .output()?;
        if !rebase.status.success() {
            let _ = std::process::Command::new("git")
                .args(["rebase", "--abort"])
                .current_dir(repo_path)
                .output();
            return Err(anyhow::anyhow!(
                "git rebase failed: {}",
                String::from_utf8_lossy(&rebase.stderr).trim()
            ));
        }
    }

    Ok(())
}

/// Returns `true` if any repo has uncommitted files that categorize as `CommitAndPush`.
///
/// Unlike `has_committable_files`, this function does NOT examine `unpushed_branches`.
/// It is used after `push_unpushed_repos` succeeds to check whether the LLM finalization
/// agent is still needed — bare-clone worktrees have no `refs/remotes/`, so
/// `inspect_unpushed` always reports branches as "not in remotes" even after a successful
/// push. Checking only uncommitted files avoids a false-positive that would otherwise force
/// every session through the 160 s LLM timeout.
pub fn has_uncommitted_committable_files(
    workspace_path: &Path,
    dirty_state: &[RepoDirtyState],
) -> bool {
    for repo in dirty_state {
        let context = build_repo_context(workspace_path, repo);
        for file in &repo.uncommitted_files {
            let file_path = extract_porcelain_path(file);
            if categorize::categorize(Path::new(file_path), &context) == Category::CommitAndPush {
                return true;
            }
        }
    }
    false
}

pub fn push_to_recovery_branch(
    _repo_path: &Path,
    session_id: &SessionId,
    original_branch: &str,
) -> Result<String> {
    Ok(format!("recovery/{}/{}", session_id, original_branch))
}

fn build_repo_context(workspace_path: &Path, repo: &RepoDirtyState) -> RepoContext {
    let repo_kind = if repo.repo_name == "team" {
        RepoKind::Team
    } else {
        RepoKind::Project
    };

    let repo_path = if repo.repo_name == "team" {
        workspace_path.join("team")
    } else {
        workspace_path.join("projects").join(&repo.repo_name)
    };

    let current_branch = get_current_branch(&repo_path).unwrap_or_else(|| "main".to_string());

    let tracking_status = if !repo.unpushed_branches.is_empty() {
        TrackingStatus::Ahead
    } else {
        TrackingStatus::Untracked
    };

    let uncommitted_files: Vec<String> = repo
        .uncommitted_files
        .iter()
        .map(|f| extract_porcelain_path(f).to_string())
        .collect();

    RepoContext {
        repo_kind,
        current_branch,
        default_branch: "main".to_string(),
        tracking_status,
        uncommitted_files,
    }
}

fn get_current_branch(repo_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        None
    } else {
        Some(branch)
    }
}

/// Extract the file path from a `git status --porcelain` line.
///
/// Porcelain format: `XY filename` — two status characters followed by
/// a space and the path. Returns the input unchanged when it doesn't
/// match porcelain format.
fn extract_porcelain_path(line: &str) -> &str {
    if line.len() > 3 && line.as_bytes()[2] == b' ' {
        &line[3..]
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::session::dirty_state::RepoDirtyState;
    use crate::session::manager::{CreateSessionParams, SessionManager, WorkspaceOps};
    use crate::session::registry::SessionRegistry;
    use crate::session::types::{SessionId, SessionRecord, SessionState, SessionType};
    use crate::session::work_item_lock::WorkItemLock;

    struct FakeWorkspaceOps {
        workspace_path: PathBuf,
    }

    impl WorkspaceOps for FakeWorkspaceOps {
        fn hydrate_workspace(&self, _session_id: &SessionId, _member: &str) -> Result<PathBuf> {
            Ok(self.workspace_path.clone())
        }

        fn inspect_dirty_state(&self, _workspace_path: &Path) -> Result<Vec<RepoDirtyState>> {
            Ok(vec![])
        }
    }

    fn make_test_manager() -> SessionManager<FakeWorkspaceOps> {
        let tmp = tempfile::tempdir().unwrap();
        let registry = SessionRegistry::new(tmp.path().join("registry.json"));
        let lock = WorkItemLock::new();
        let ops = FakeWorkspaceOps {
            workspace_path: tmp.path().join("workspace"),
        };
        SessionManager::new(registry, lock, ops)
    }

    fn dirty_repo(name: &str, uncommitted: &[&str], unpushed: &[&str]) -> RepoDirtyState {
        RepoDirtyState {
            repo_name: name.to_string(),
            uncommitted_files: uncommitted.iter().map(|s| s.to_string()).collect(),
            unpushed_branches: unpushed.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn clean_repo(name: &str) -> RepoDirtyState {
        dirty_repo(name, &[], &[])
    }

    fn git(dir: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn setup_project_workspace(tmp: &Path, repo_name: &str, branch: &str) -> PathBuf {
        let ws = tmp.join("workspace");
        let repo = ws.join("projects").join(repo_name);
        std::fs::create_dir_all(&repo).unwrap();

        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@test.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("README.md"), "init").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "initial"]);

        if branch != "main" {
            git(&repo, &["checkout", "-b", branch]);
        }

        ws
    }

    // ---
    // has_committable_files — production caller for categorize()
    // ---

    #[test]
    fn committable_files_project_on_feature_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "feature/story-88");
        let dirty = vec![dirty_repo("myproject", &["src/lib.rs"], &[])];

        assert!(
            has_committable_files(&ws, &dirty),
            "uncommitted file in project repo on feature branch must be committable"
        );
    }

    #[test]
    fn committable_files_project_on_default_branch_returns_false() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "main");
        let dirty = vec![dirty_repo("myproject", &["src/lib.rs"], &[])];

        assert!(
            !has_committable_files(&ws, &dirty),
            "uncommitted file in project repo on default branch must not be committable"
        );
    }

    #[test]
    fn committable_files_team_specs() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let dirty = vec![dirty_repo(
            "team",
            &["specs/epic-85/design.md", "knowledge/patterns.md"],
            &[],
        )];

        assert!(
            has_committable_files(&ws, &dirty),
            "uncommitted team repo files under specs/ or knowledge/ must be committable"
        );
    }

    #[test]
    fn committable_files_clean_repos_returns_false() {
        let tmp = tempfile::tempdir().unwrap();
        let dirty = vec![clean_repo("myproject")];

        assert!(
            !has_committable_files(tmp.path(), &dirty),
            "clean repos must not have committable files"
        );
    }

    #[test]
    fn committable_files_credentials_not_committable() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "feature/story-88");
        let dirty = vec![dirty_repo("myproject", &[".env", "token.txt"], &[])];

        assert!(
            !has_committable_files(&ws, &dirty),
            "credential files must not be committable"
        );
    }

    #[test]
    fn committable_files_unpushed_branches_trigger_finalization() {
        let tmp = tempfile::tempdir().unwrap();
        let dirty = vec![dirty_repo(
            "myproject",
            &[],
            &["abc123 feat: committed but not pushed"],
        )];

        assert!(
            has_committable_files(tmp.path(), &dirty),
            "repos with committed-but-unpushed branches must trigger finalization"
        );
    }

    #[test]
    fn committable_files_team_member_knowledge() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let dirty = vec![dirty_repo(
            "team",
            &["members/bob/knowledge/notes.md"],
            &[],
        )];

        assert!(
            has_committable_files(&ws, &dirty),
            "uncommitted files under members/*/knowledge/ in team repo must be committable"
        );
    }

    #[test]
    fn committable_files_porcelain_format() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "feature/story-88");
        let dirty = vec![dirty_repo("myproject", &[" M src/lib.rs"], &[])];

        assert!(
            has_committable_files(&ws, &dirty),
            "porcelain-format file paths must be handled correctly"
        );
    }

    // ---
    // finalize_session
    // ---

    #[test]
    fn dirty_session_on_feature_branch_triggers_finalization() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "feature/story-88");
        let session_id = SessionId::from_raw("abc12345");
        let dirty = vec![dirty_repo("myproject", &["src/lib.rs"], &[])];

        let result = finalize_session(&session_id, &ws, &dirty);

        assert_ne!(
            result.outcome,
            FinalizationOutcome::Skipped,
            "dirty session with committable files must not skip finalization"
        );
    }

    #[test]
    fn clean_session_finalization_returns_skipped() {
        let session_id = SessionId::from_raw("abc12345");
        let tmp = tempfile::tempdir().unwrap();
        let dirty = vec![clean_repo("myproject")];

        let result = finalize_session(&session_id, tmp.path(), &dirty);

        assert_eq!(
            result.outcome,
            FinalizationOutcome::Skipped,
            "clean session must skip finalization"
        );
    }

    #[test]
    fn empty_dirty_state_finalization_returns_skipped() {
        let session_id = SessionId::from_raw("abc12345");
        let tmp = tempfile::tempdir().unwrap();

        let result = finalize_session(&session_id, tmp.path(), &[]);

        assert_eq!(
            result.outcome,
            FinalizationOutcome::Skipped,
            "empty dirty state must skip finalization"
        );
    }

    #[test]
    fn unpushed_only_session_finalization_triggers() {
        let session_id = SessionId::from_raw("abc12345");
        let tmp = tempfile::tempdir().unwrap();
        let dirty = vec![dirty_repo("myproject", &[], &["feature/story-88"])];

        let result = finalize_session(&session_id, tmp.path(), &dirty);

        assert_ne!(
            result.outcome,
            FinalizationOutcome::Skipped,
            "session with unpushed branches must trigger finalization — \
             push_and_refresh_dirty runs first and clears branches on success; \
             remaining unpushed branches indicate push failure, requiring the subagent"
        );
    }

    #[test]
    fn team_repo_memories_trigger_finalization() {
        let session_id = SessionId::from_raw("abc12345");
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let dirty = vec![dirty_repo(
            "team",
            &[
                "specs/epic-85/design.md",
                "knowledge/patterns.md",
                "members/bob/knowledge/notes.md",
            ],
            &[],
        )];

        let result = finalize_session(&session_id, &ws, &dirty);

        assert_ne!(
            result.outcome,
            FinalizationOutcome::Skipped,
            "team repo with uncommitted memories must trigger finalization"
        );
    }

    #[test]
    fn project_on_default_branch_skips_finalization() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "main");
        let session_id = SessionId::from_raw("abc12345");
        let dirty = vec![dirty_repo("myproject", &["src/lib.rs"], &[])];

        let result = finalize_session(&session_id, &ws, &dirty);

        assert_eq!(
            result.outcome,
            FinalizationOutcome::Skipped,
            "project on default branch must skip finalization \
             — uncommitted files are left in place"
        );
    }

    // ---
    // push_unpushed_repos
    // ---

    /// Set up a local bare repo and a clone with an unpushed branch.
    /// Returns (workspace_root, bare_remote_path, repo_name).
    fn setup_push_workspace(tmp: &Path, repo_name: &str, branch: &str) -> (PathBuf, PathBuf) {
        let bare = tmp.join(format!("{repo_name}.git"));
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--bare", "-b", "main"]);

        let ws = tmp.join("workspace");
        let repo_path = ws.join("projects").join(repo_name);
        std::fs::create_dir_all(&repo_path).unwrap();

        git(&repo_path, &["init", "-b", "main"]);
        git(&repo_path, &["config", "user.email", "test@test.com"]);
        git(&repo_path, &["config", "user.name", "Test"]);
        git(&repo_path, &["config", "commit.gpgsign", "false"]);

        // Initial commit on main and push it
        std::fs::write(repo_path.join("README.md"), "init").unwrap();
        git(&repo_path, &["add", "."]);
        git(&repo_path, &["commit", "-m", "initial"]);
        git(&repo_path, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&repo_path, &["push", "origin", "main"]);

        // Create feature branch with a commit
        if branch != "main" {
            git(&repo_path, &["checkout", "-b", branch]);
            std::fs::write(repo_path.join("feature.txt"), "work").unwrap();
            git(&repo_path, &["add", "."]);
            git(&repo_path, &["commit", "-m", "feat: add feature"]);
        }

        (ws, bare)
    }

    #[test]
    fn push_unpushed_repos_succeeds_for_project_with_unpushed_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, _bare) =
            setup_push_workspace(tmp.path(), "myproject", "feature/story-99");

        let dirty = vec![dirty_repo(
            "myproject",
            &[],
            &["feature/story-99 feat: add feature"],
        )];

        let result = push_unpushed_repos(&ws, &dirty);

        assert!(
            result.is_ok(),
            "push_unpushed_repos must succeed when remote is reachable: {:?}",
            result.err()
        );
    }

    #[test]
    fn push_unpushed_repos_skips_repos_with_no_unpushed_branches() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();

        let dirty = vec![dirty_repo("myproject", &["src/lib.rs"], &[])];

        // No git repo exists at workspace/projects/myproject — if we tried to push,
        // git would fail. Passing means the function correctly skipped it.
        let result = push_unpushed_repos(&ws, &dirty);

        assert!(
            result.is_ok(),
            "push_unpushed_repos must skip repos with no unpushed branches"
        );
    }

    #[test]
    fn push_unpushed_repos_returns_err_when_remote_is_unreachable() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        let repo_path = ws.join("projects").join("myproject");
        std::fs::create_dir_all(&repo_path).unwrap();

        // Repo with a remote pointing to a non-existent path
        git(&repo_path, &["init", "-b", "main"]);
        git(&repo_path, &["config", "user.email", "test@test.com"]);
        git(&repo_path, &["config", "user.name", "Test"]);
        git(&repo_path, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo_path.join("file.txt"), "content").unwrap();
        git(&repo_path, &["add", "."]);
        git(&repo_path, &["commit", "-m", "initial"]);
        git(&repo_path, &["remote", "add", "origin", "/nonexistent/bare.git"]);

        let dirty = vec![dirty_repo(
            "myproject",
            &[],
            &["main feat: some commit"],
        )];

        let result = push_unpushed_repos(&ws, &dirty);

        assert!(
            result.is_err(),
            "push_unpushed_repos must return Err when git push fails"
        );
    }

    #[test]
    fn push_unpushed_repos_empty_dirty_state_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        std::fs::create_dir_all(&ws).unwrap();

        let result = push_unpushed_repos(&ws, &[]);

        assert!(
            result.is_ok(),
            "push_unpushed_repos on empty dirty state must be a noop"
        );
    }

    #[test]
    fn push_unpushed_repos_skips_detached_head() {
        // Bare-clone worktrees provisioned with `--detach` start in detached HEAD state.
        // push_unpushed_repos must skip these repos rather than fail.
        let tmp = tempfile::tempdir().unwrap();
        let (ws, _bare) = setup_push_workspace(tmp.path(), "myproject", "main");
        let repo_path = ws.join("projects").join("myproject");

        // Force detached HEAD by checking out the commit hash directly.
        let hash = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_path)
            .output()
            .unwrap();
        let hash_str = String::from_utf8_lossy(&hash.stdout).trim().to_string();
        git(&repo_path, &["checkout", "--detach", &hash_str]);

        let dirty = vec![dirty_repo("myproject", &[], &["main abc123 initial"])];

        let result = push_unpushed_repos(&ws, &dirty);

        assert!(
            result.is_ok(),
            "push_unpushed_repos must skip detached HEAD repos: {:?}",
            result.err()
        );
    }

    // ---
    // has_uncommitted_committable_files
    // ---

    #[test]
    fn uncommitted_committable_files_project_on_feature_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "feature/story-88");
        let dirty = vec![dirty_repo("myproject", &["src/lib.rs"], &[])];

        assert!(
            has_uncommitted_committable_files(&ws, &dirty),
            "uncommitted file on feature branch must be detected as committable"
        );
    }

    #[test]
    fn uncommitted_committable_files_ignores_unpushed_branches() {
        // This is the key invariant: after push_unpushed_repos succeeds, bare-clone
        // worktrees still show branches as "not in remotes". has_uncommitted_committable_files
        // must return false so the session correctly reaches Completed.
        let tmp = tempfile::tempdir().unwrap();
        let dirty = vec![dirty_repo(
            "myproject",
            &[],
            &["abc123 feat: committed but not pushed"],
        )];

        assert!(
            !has_uncommitted_committable_files(tmp.path(), &dirty),
            "unpushed_branches alone must not trigger LLM agent after a successful push"
        );
    }

    #[test]
    fn uncommitted_committable_files_project_on_default_branch_returns_false() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = setup_project_workspace(tmp.path(), "myproject", "main");
        let dirty = vec![dirty_repo("myproject", &["src/lib.rs"], &[])];

        assert!(
            !has_uncommitted_committable_files(&ws, &dirty),
            "uncommitted file on default branch must not trigger LLM agent"
        );
    }

    #[test]
    fn uncommitted_committable_files_clean_repos_returns_false() {
        let tmp = tempfile::tempdir().unwrap();
        let dirty = vec![clean_repo("myproject")];

        assert!(
            !has_uncommitted_committable_files(tmp.path(), &dirty),
            "clean repos must not trigger LLM agent"
        );
    }

    // ---
    // push_to_recovery_branch
    // ---

    #[test]
    fn push_to_recovery_branch_succeeds() {
        let session_id = SessionId::from_raw("abc12345");
        let tmp = tempfile::tempdir().unwrap();

        let result = push_to_recovery_branch(tmp.path(), &session_id, "feature/story-88");

        assert!(
            result.is_ok(),
            "push_to_recovery_branch must succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn recovery_branch_name_follows_convention() {
        let session_id = SessionId::from_raw("abc12345");
        let tmp = tempfile::tempdir().unwrap();

        let branch = push_to_recovery_branch(tmp.path(), &session_id, "main")
            .expect("push_to_recovery_branch must succeed");

        assert_eq!(
            branch, "recovery/abc12345/main",
            "recovery branch must follow recovery/<session-id>/<branch> convention"
        );
    }

    // ---
    // retrigger_finalization
    // ---

    #[test]
    fn retrigger_delegates_to_subagent() {
        let session_id = SessionId::from_raw("abc12345");
        let workspace = PathBuf::from("/nonexistent/deactivation-retrigger-test");

        let result = retrigger_finalization(&session_id, &workspace);

        assert!(
            result.is_err(),
            "retrigger_finalization with non-existent workspace must fail — \
             confirms delegation to subagent (not a stub)"
        );
    }

    // ---
    // State transition tests
    // ---

    #[test]
    fn retained_to_finalizing_is_valid_transition() {
        assert!(
            SessionState::Retained.can_transition_to(&SessionState::Finalizing),
            "Retained -> Finalizing must be valid to support re-trigger finalization"
        );
    }

    #[test]
    fn new_session_while_old_is_finalizing() {
        let mut mgr = make_test_manager();

        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws1")),
            finalization_result: None,
                finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Finalizing)
            .unwrap();

        let params = CreateSessionParams {
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            work_item_id: None,
        };
        let result = mgr.create_session(params);

        assert!(
            result.is_ok(),
            "new session must succeed when old session is Finalizing"
        );
    }

    #[test]
    fn new_session_while_old_is_failed() {
        let mut mgr = make_test_manager();

        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "alice".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws1")),
            finalization_result: None,
                finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Failed)
            .unwrap();

        let params = CreateSessionParams {
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            work_item_id: None,
        };
        let result = mgr.create_session(params);

        assert!(
            result.is_ok(),
            "new session must succeed when old session is Failed"
        );
    }

    // ---
    // extract_porcelain_path
    // ---

    #[test]
    fn extract_porcelain_modified_file() {
        assert_eq!(extract_porcelain_path(" M src/lib.rs"), "src/lib.rs");
    }

    #[test]
    fn extract_porcelain_untracked_file() {
        assert_eq!(extract_porcelain_path("?? new_file.rs"), "new_file.rs");
    }

    #[test]
    fn extract_porcelain_added_file() {
        assert_eq!(extract_porcelain_path("A  src/new.rs"), "src/new.rs");
    }

    #[test]
    fn extract_porcelain_clean_path_unchanged() {
        assert_eq!(extract_porcelain_path("src/lib.rs"), "src/lib.rs");
    }
}
