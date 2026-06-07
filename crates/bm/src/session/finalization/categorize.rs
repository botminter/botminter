use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    CommitAndPush,
    PushOnly,
    NeverCommit,
    LeaveInPlace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoKind {
    Project,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingStatus {
    Ahead,
    Behind,
    Diverged,
    Untracked,
}

#[derive(Debug, Clone)]
pub struct RepoContext {
    pub repo_kind: RepoKind,
    pub current_branch: String,
    pub default_branch: String,
    pub tracking_status: TrackingStatus,
    pub uncommitted_files: Vec<String>,
}

pub fn categorize(file_path: &Path, context: &RepoContext) -> Category {
    let path_str = file_path.to_string_lossy();
    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();

    if is_credential_path(&path_str, &file_name) {
        return Category::NeverCommit;
    }

    if path_str.starts_with(".ralph/") {
        return Category::LeaveInPlace;
    }

    let is_uncommitted = context
        .uncommitted_files
        .iter()
        .any(|f| f.as_str() == path_str.as_ref());

    if is_uncommitted {
        match context.repo_kind {
            RepoKind::Project => {
                if context.current_branch != context.default_branch {
                    return Category::CommitAndPush;
                }
            }
            RepoKind::Team => {
                if is_team_committable_path(&path_str) {
                    return Category::CommitAndPush;
                }
            }
        }
    } else if context.tracking_status == TrackingStatus::Ahead {
        return Category::PushOnly;
    }

    Category::LeaveInPlace
}

fn is_credential_path(path_str: &str, file_name: &str) -> bool {
    path_str.contains(".config/gh/")
        || file_name == ".env"
        || file_name.starts_with(".env.")
        || file_name == "token.txt"
}

fn is_team_committable_path(path: &str) -> bool {
    path.starts_with("specs/")
        || path.starts_with("knowledge/")
        || is_member_knowledge_path(path)
}

fn is_member_knowledge_path(path: &str) -> bool {
    if let Some(rest) = path.strip_prefix("members/") {
        if let Some(slash_pos) = rest.find('/') {
            let after_member = &rest[slash_pos + 1..];
            return after_member.starts_with("knowledge/") || after_member == "knowledge";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn project_context_on_feature_branch() -> RepoContext {
        RepoContext {
            repo_kind: RepoKind::Project,
            current_branch: "feature/story-88".to_string(),
            default_branch: "main".to_string(),
            tracking_status: TrackingStatus::Ahead,
            uncommitted_files: vec!["src/lib.rs".to_string(), "src/new.rs".to_string()],
        }
    }

    fn project_context_on_default_branch() -> RepoContext {
        RepoContext {
            repo_kind: RepoKind::Project,
            current_branch: "main".to_string(),
            default_branch: "main".to_string(),
            tracking_status: TrackingStatus::Ahead,
            uncommitted_files: vec![],
        }
    }

    fn team_context() -> RepoContext {
        RepoContext {
            repo_kind: RepoKind::Team,
            current_branch: "main".to_string(),
            default_branch: "main".to_string(),
            tracking_status: TrackingStatus::Ahead,
            uncommitted_files: vec![
                "specs/epic-85/design.md".to_string(),
                "knowledge/patterns.md".to_string(),
            ],
        }
    }

    fn context_with_unpushed_only() -> RepoContext {
        RepoContext {
            repo_kind: RepoKind::Project,
            current_branch: "feature/story-88".to_string(),
            default_branch: "main".to_string(),
            tracking_status: TrackingStatus::Ahead,
            uncommitted_files: vec![],
        }
    }

    // AC-1: Project repo uncommitted changes on non-default branch → CommitAndPush

    #[test]
    fn project_uncommitted_on_feature_branch_is_commit_and_push() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new("src/lib.rs"), &ctx);
        assert_eq!(
            result,
            Category::CommitAndPush,
            "uncommitted file in project repo on feature branch must be CommitAndPush"
        );
    }

    #[test]
    fn project_uncommitted_new_file_on_feature_branch_is_commit_and_push() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new("src/new.rs"), &ctx);
        assert_eq!(
            result,
            Category::CommitAndPush,
            "new uncommitted file in project repo on feature branch must be CommitAndPush"
        );
    }

    // AC-2: Team repo uncommitted files under specs/ or knowledge/ → CommitAndPush

    #[test]
    fn team_uncommitted_under_specs_is_commit_and_push() {
        let ctx = team_context();
        let result = categorize(Path::new("specs/epic-85/design.md"), &ctx);
        assert_eq!(
            result,
            Category::CommitAndPush,
            "uncommitted file under specs/ in team repo must be CommitAndPush"
        );
    }

    #[test]
    fn team_uncommitted_under_knowledge_is_commit_and_push() {
        let ctx = team_context();
        let result = categorize(Path::new("knowledge/patterns.md"), &ctx);
        assert_eq!(
            result,
            Category::CommitAndPush,
            "uncommitted file under knowledge/ in team repo must be CommitAndPush"
        );
    }

    #[test]
    fn team_uncommitted_under_members_knowledge_is_commit_and_push() {
        let mut ctx = team_context();
        ctx.uncommitted_files
            .push("members/bob/knowledge/notes.md".to_string());
        let result = categorize(Path::new("members/bob/knowledge/notes.md"), &ctx);
        assert_eq!(
            result,
            Category::CommitAndPush,
            "uncommitted file under members/*/knowledge/ in team repo must be CommitAndPush"
        );
    }

    #[test]
    fn team_uncommitted_outside_specs_knowledge_is_leave_in_place() {
        let mut ctx = team_context();
        ctx.uncommitted_files.push("README.md".to_string());
        let result = categorize(Path::new("README.md"), &ctx);
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "uncommitted file outside specs/knowledge in team repo must be LeaveInPlace"
        );
    }

    // AC-3: Committed-but-unpushed changes → PushOnly

    #[test]
    fn committed_unpushed_file_is_push_only() {
        let ctx = context_with_unpushed_only();
        let result = categorize(Path::new("src/lib.rs"), &ctx);
        assert_eq!(
            result,
            Category::PushOnly,
            "committed-but-unpushed file must be PushOnly"
        );
    }

    #[test]
    fn committed_unpushed_on_default_branch_is_push_only() {
        let mut ctx = project_context_on_default_branch();
        ctx.tracking_status = TrackingStatus::Ahead;
        let result = categorize(Path::new("src/lib.rs"), &ctx);
        assert_eq!(
            result,
            Category::PushOnly,
            "committed-but-unpushed file on default branch must be PushOnly"
        );
    }

    // AC-4: Credential paths → NeverCommit

    #[test]
    fn config_gh_path_is_never_commit() {
        let mut ctx = project_context_on_feature_branch();
        ctx.uncommitted_files
            .push(".config/gh/hosts.yml".to_string());
        let result = categorize(Path::new(".config/gh/hosts.yml"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            ".config/gh/ files must be NeverCommit"
        );
    }

    #[test]
    fn config_gh_nested_file_is_never_commit() {
        let mut ctx = project_context_on_feature_branch();
        ctx.uncommitted_files
            .push(".config/gh/config.yml".to_string());
        let result = categorize(Path::new(".config/gh/config.yml"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            "any file under .config/gh/ must be NeverCommit"
        );
    }

    #[test]
    fn env_file_is_never_commit() {
        let mut ctx = project_context_on_feature_branch();
        ctx.uncommitted_files.push(".env".to_string());
        let result = categorize(Path::new(".env"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            ".env files must be NeverCommit"
        );
    }

    #[test]
    fn env_local_file_is_never_commit() {
        let mut ctx = project_context_on_feature_branch();
        ctx.uncommitted_files.push(".env.local".to_string());
        let result = categorize(Path::new(".env.local"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            ".env.local files must be NeverCommit"
        );
    }

    #[test]
    fn token_file_is_never_commit() {
        let mut ctx = project_context_on_feature_branch();
        ctx.uncommitted_files.push("token.txt".to_string());
        let result = categorize(Path::new("token.txt"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            "token files must be NeverCommit"
        );
    }

    // AC-5: Runtime artifacts → LeaveInPlace

    #[test]
    fn ralph_log_is_leave_in_place() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(
            Path::new(".ralph/diagnostics/logs/ralph-2026-01-01.log"),
            &ctx,
        );
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "log files must be LeaveInPlace"
        );
    }

    #[test]
    fn lock_file_is_leave_in_place() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new(".ralph/loop.lock"), &ctx);
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "lock files must be LeaveInPlace"
        );
    }

    #[test]
    fn tasks_jsonl_is_leave_in_place() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new(".ralph/agent/tasks.jsonl"), &ctx);
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "tasks.jsonl must be LeaveInPlace"
        );
    }

    #[test]
    fn summary_md_is_leave_in_place() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new(".ralph/agent/summary.md"), &ctx);
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "summary.md must be LeaveInPlace"
        );
    }

    #[test]
    fn history_jsonl_is_leave_in_place() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new(".ralph/history.jsonl"), &ctx);
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "history.jsonl must be LeaveInPlace"
        );
    }

    #[test]
    fn event_files_are_leave_in_place() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new(".ralph/events-20260101-120000.jsonl"), &ctx);
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "event files must be LeaveInPlace"
        );
    }

    #[test]
    fn diagnostics_dir_is_leave_in_place() {
        let ctx = project_context_on_feature_branch();
        let result = categorize(Path::new(".ralph/diagnostics/some-report.txt"), &ctx);
        assert_eq!(
            result,
            Category::LeaveInPlace,
            "diagnostics files must be LeaveInPlace"
        );
    }

    // AC-6: Overlap rule — NeverCommit wins over CommitAndPush

    #[test]
    fn env_under_members_knowledge_is_never_commit() {
        let mut ctx = team_context();
        ctx.uncommitted_files
            .push("members/bob/knowledge/.env".to_string());
        let result = categorize(Path::new("members/bob/knowledge/.env"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            "NeverCommit must win over CommitAndPush for credential files in knowledge dirs"
        );
    }

    #[test]
    fn gh_config_under_specs_is_never_commit() {
        let mut ctx = team_context();
        ctx.uncommitted_files
            .push("specs/.config/gh/hosts.yml".to_string());
        let result = categorize(Path::new("specs/.config/gh/hosts.yml"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            "NeverCommit must win over CommitAndPush for .config/gh/ under specs"
        );
    }

    #[test]
    fn token_under_knowledge_is_never_commit() {
        let mut ctx = team_context();
        ctx.uncommitted_files
            .push("knowledge/token.txt".to_string());
        let result = categorize(Path::new("knowledge/token.txt"), &ctx);
        assert_eq!(
            result,
            Category::NeverCommit,
            "NeverCommit must win over CommitAndPush for token files in knowledge dirs"
        );
    }
}
