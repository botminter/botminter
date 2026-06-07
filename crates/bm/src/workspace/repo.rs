use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::profile::CodingAgentDef;
use super::sync::write_workspace_marker;
use super::util::{git_cmd, git_submodule_add, symlink_md_files, symlink_subdirs};

// ── Remote repo abstraction ─────────────────────────────────────────

/// State of a remote workspace repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRepoState {
    /// Repo does not exist on the remote.
    NotFound,
    /// Repo exists but has no commits (push never succeeded).
    Empty,
    /// Repo exists and has commits (push succeeded at least once).
    HasContent,
}

/// Abstracts remote repo operations for testability.
pub trait RemoteRepoOps {
    /// Check repo state: not found, empty, or has content.
    fn repo_state(&self, repo_name: &str) -> Result<RemoteRepoState>;
    /// Create a new private repo.
    fn create_repo(&self, repo_name: &str) -> Result<()>;
    /// Delete a repo (best-effort).
    fn delete_repo(&self, repo_name: &str) -> Result<()>;
    /// Clone a repo to a local path. If `recursive` is true, initialise submodules.
    fn clone_repo(&self, repo_name: &str, target: &Path, recursive: bool) -> Result<()>;
    /// Push from a local path to the remote.
    fn push_repo(&self, local_path: &Path) -> Result<()>;
}

/// Production implementation backed by `gh` and `git` CLI calls.
pub struct GhRemoteOps {
    pub gh_token: String,
}

impl RemoteRepoOps for GhRemoteOps {
    fn repo_state(&self, repo_name: &str) -> Result<RemoteRepoState> {
        // First check if the repo exists at all
        let view = Command::new("gh")
            .args(["repo", "view", repo_name, "--json", "name"])
            .env("GH_TOKEN", &self.gh_token)
            .output()
            .context("Failed to run `gh repo view`")?;

        if !view.status.success() {
            return Ok(RemoteRepoState::NotFound);
        }

        // Repo exists — check if it has any commits via git ls-remote
        let url = format!("https://github.com/{}.git", repo_name);
        let ls = Command::new("git")
            .args(["ls-remote", "--heads", &url])
            .env("GH_TOKEN", &self.gh_token)
            .output()
            .context("Failed to run `git ls-remote`")?;

        if ls.status.success() {
            let stdout = String::from_utf8_lossy(&ls.stdout);
            if stdout.trim().is_empty() {
                Ok(RemoteRepoState::Empty)
            } else {
                Ok(RemoteRepoState::HasContent)
            }
        } else {
            // ls-remote failed but repo exists — treat as empty
            Ok(RemoteRepoState::Empty)
        }
    }

    fn create_repo(&self, repo_name: &str) -> Result<()> {
        let output = Command::new("gh")
            .args(["repo", "create", repo_name, "--private"])
            .env("GH_TOKEN", &self.gh_token)
            .output()
            .context("Failed to run `gh repo create`")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "Failed to create workspace repo '{}'.\n{}\n\n\
                 If the repo already exists:\n  \
                 gh repo delete {} --yes\n\
                 Then re-run `bm teams sync --repos`.",
                repo_name,
                stderr.trim(),
                repo_name,
            );
        }
        Ok(())
    }

    fn delete_repo(&self, repo_name: &str) -> Result<()> {
        let _ = Command::new("gh")
            .args(["repo", "delete", repo_name, "--yes"])
            .env("GH_TOKEN", &self.gh_token)
            .output();
        // Best-effort — ignore errors
        Ok(())
    }

    fn clone_repo(&self, repo_name: &str, target: &Path, recursive: bool) -> Result<()> {
        let url = format!("https://github.com/{}.git", repo_name);
        let target_str = target.to_string_lossy().to_string();
        let parent = target.parent().unwrap_or(Path::new("."));

        let mut args = vec!["clone"];
        if recursive {
            args.push("--recursive");
        }
        args.push(&url);
        args.push(&target_str);

        git_cmd(parent, &args).with_context(|| {
            format!(
                "Failed to clone workspace repo {}\n\n\
                 To verify: gh repo view {}",
                repo_name, repo_name,
            )
        })
    }

    fn push_repo(&self, local_path: &Path) -> Result<()> {
        git_cmd(local_path, &["push", "-u", "origin", "main"])
    }
}

/// Parameters for creating a workspace repo with submodules.
pub struct WorkspaceRepoParams<'a> {
    pub team_repo_path: &'a Path,
    pub workspace_base: &'a Path,
    pub member_dir_name: &'a str,
    pub team_name: &'a str,
    pub projects: &'a [(&'a str, &'a str)], // [(project_name, fork_url)]
    pub github_repo: Option<&'a str>,
    pub project_number: Option<u64>,
    pub push: bool,
    pub coding_agent: &'a CodingAgentDef,
    /// Remote repo operations. Required when `push` is true.
    /// Pass `None` for local-only mode.
    pub remote_ops: Option<&'a dyn RemoteRepoOps>,
    /// Override the team submodule URL (for testing). When `None`, derived from
    /// `github_repo` (push mode) or `team_repo_path` (local mode).
    pub team_submodule_url: Option<&'a str>,
}

/// Creates a workspace repo for a member using the submodule model.
///
/// This replaces the old `.botminter/` clone model. The workspace is a git repo
/// containing submodules: `team/` points to the team repo, and `projects/<name>/`
/// points to project forks. Member branches are checked out in all submodules.
///
/// When `push` is true (i.e., `bm teams sync --repos`), a GitHub repo is
/// created via `gh repo create`. When false, the workspace is local-only.
///
/// ## Idempotency (push mode)
///
/// `git push` is the atomic commit point. The remote repo state determines
/// the recovery strategy:
///
/// - **HasContent**: push previously succeeded — clone (with `--recursive`)
///   and return early. The workspace is already complete on the remote.
/// - **Empty**: push never succeeded — the remote is inconsistent. Delete it,
///   re-create, and proceed with full assembly + push.
/// - **NotFound**: fresh start — create and proceed normally.
pub fn create_workspace_repo(params: &WorkspaceRepoParams) -> Result<()> {
    let member_ws = params.workspace_base.join(params.member_dir_name);

    if params.push {
        // Extract org from github_repo (e.g., "myorg/my-team" → "myorg")
        let org = params
            .github_repo
            .and_then(|r| r.split('/').next())
            .unwrap_or("");
        if org.is_empty() {
            bail!(
                "Cannot create workspace repo: no GitHub org found.\n\
                 The team must have a github_repo configured (e.g., 'myorg/my-team')."
            );
        }

        let remote = params.remote_ops.ok_or_else(|| {
            anyhow::anyhow!("remote_ops is required when push=true")
        })?;

        let ws_repo_name = format!("{}/{}-{}", org, params.team_name, params.member_dir_name);

        match remote.repo_state(&ws_repo_name)? {
            RemoteRepoState::HasContent => {
                // Push previously succeeded — clone and return early
                if member_ws.exists() {
                    fs::remove_dir_all(&member_ws).ok();
                }
                remote.clone_repo(&ws_repo_name, &member_ws, true)?;
                return Ok(());
            }
            RemoteRepoState::Empty => {
                // Push never succeeded — inconsistent state. Clean up and re-create.
                if member_ws.exists() {
                    fs::remove_dir_all(&member_ws).ok();
                }
                remote.delete_repo(&ws_repo_name)?;
                remote.create_repo(&ws_repo_name)?;
                wait_for_repo(remote, &ws_repo_name)?;
                remote.clone_repo(&ws_repo_name, &member_ws, false)?;
            }
            RemoteRepoState::NotFound => {
                // Fresh — create and clone
                if member_ws.exists() {
                    fs::remove_dir_all(&member_ws).ok();
                }
                remote.create_repo(&ws_repo_name)?;
                wait_for_repo(remote, &ws_repo_name)?;
                remote.clone_repo(&ws_repo_name, &member_ws, false)?;
            }
        }
        // Fall through to assemble context files, commit, and push.
    } else {
        // Local-only mode: git init
        fs::create_dir_all(&member_ws)
            .with_context(|| format!("Failed to create workspace dir {}", member_ws.display()))?;
        git_cmd(&member_ws, &["init", "-b", "main"])?;
    }

    // Configure git user for the workspace repo
    git_cmd(&member_ws, &["config", "user.email", "botminter@local"])?;
    git_cmd(&member_ws, &["config", "user.name", "BotMinter"])?;

    // Add team repo as submodule at `team/`
    let team_repo_url = if let Some(url) = params.team_submodule_url {
        // Explicit override (used by tests)
        url.to_string()
    } else if params.push {
        // Use the GitHub URL for the team repo
        params
            .github_repo
            .map(|r| format!("https://github.com/{}.git", r))
            .unwrap_or_else(|| {
                fs::canonicalize(params.team_repo_path)
                    .unwrap_or_else(|_| params.team_repo_path.to_path_buf())
                    .to_string_lossy()
                    .to_string()
            })
    } else {
        // Use local path for the team repo
        fs::canonicalize(params.team_repo_path)
            .unwrap_or_else(|_| params.team_repo_path.to_path_buf())
            .to_string_lossy()
            .to_string()
    };

    git_submodule_add(&member_ws, &team_repo_url, "team")
        .with_context(|| {
            format!(
                "Failed to add team repo submodule.\n\n\
                 To verify the team repo: git ls-remote {}",
                team_repo_url
            )
        })?;

    // Checkout member branch in team submodule
    let team_sub = member_ws.join("team");
    if git_cmd(&team_sub, &["checkout", params.member_dir_name]).is_err() {
        git_cmd(&team_sub, &["checkout", "-b", params.member_dir_name])?;
    }

    // Add project submodules
    if !params.projects.is_empty() {
        let projects_dir = member_ws.join("projects");
        fs::create_dir_all(&projects_dir)
            .with_context(|| format!("Failed to create projects dir {}", projects_dir.display()))?;

        for &(project_name, fork_url) in params.projects {
            let submodule_path = format!("projects/{}", project_name);
            git_submodule_add(&member_ws, fork_url, &submodule_path)
                .with_context(|| {
                    format!(
                        "Failed to add project submodule '{}' from {}\n\n\
                         To verify the fork: gh repo view {}",
                        project_name, fork_url, fork_url
                    )
                })?;

            // Checkout member branch in project submodule
            let proj_sub = member_ws.join("projects").join(project_name);
            if git_cmd(&proj_sub, &["checkout", params.member_dir_name]).is_err() {
                git_cmd(&proj_sub, &["checkout", "-b", params.member_dir_name])?;
            }
        }
    }

    // Assemble context files, agent dir, gitignore, and marker
    let project_names: Vec<&str> = params.projects.iter().map(|(name, _)| *name).collect();
    assemble_workspace_repo_context(
        &member_ws,
        params.member_dir_name,
        &project_names,
        params.coding_agent,
        params.github_repo,
        params.project_number,
    )?;

    // Commit workspace files (idempotent — skips if nothing changed)
    git_cmd(&member_ws, &["add", "-A"])?;
    let has_changes = git_cmd(&member_ws, &["diff", "--cached", "--quiet"]).is_err();
    if has_changes {
        git_cmd(
            &member_ws,
            &["commit", "-m", "Initial workspace setup"],
        )?;
    }

    // Push if remote is configured
    if params.push {
        if let Some(remote) = params.remote_ops {
            remote.push_repo(&member_ws)?;
        }
    }

    Ok(())
}

/// Polls `repo_state()` until the repo is visible (not `NotFound`).
/// GitHub repo creation has a propagation delay — the repo may not be
/// immediately available for git operations after `gh repo create` returns.
fn wait_for_repo(remote: &dyn RemoteRepoOps, repo_name: &str) -> Result<()> {
    for attempt in 1..=30 {
        match remote.repo_state(repo_name)? {
            RemoteRepoState::NotFound => {
                if attempt == 30 {
                    bail!(
                        "Repo '{}' not available after 15s. GitHub may be experiencing delays.",
                        repo_name
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            _ => return Ok(()),
        }
    }
    Ok(())
}

/// Assembles workspace context files, agent dir, gitignore, and marker for the
/// submodule-based workspace model.
///
/// Context files (context_file, PROMPT.md, ralph.yml) are **copied** from the
/// team submodule — they're tracked first-class citizens in the workspace repo.
/// Agent dir entries are **symlinked** into the team submodule paths.
pub fn assemble_workspace_repo_context(
    ws_root: &Path,
    member_dir_name: &str,
    project_names: &[&str],
    coding_agent: &CodingAgentDef,
    github_repo: Option<&str>,
    project_number: Option<u64>,
) -> Result<()> {
    let team_sub = ws_root.join("team");
    let member_src = team_sub.join("members").join(member_dir_name);

    // Copy context files from team/members/<member>/ to workspace root
    let context_src = member_src.join(&coding_agent.context_file);
    if context_src.exists() {
        fs::copy(&context_src, ws_root.join(&coding_agent.context_file))
            .with_context(|| format!("Failed to copy {}", coding_agent.context_file))?;
    }

    let prompt_src = member_src.join("PROMPT.md");
    if prompt_src.exists() {
        fs::copy(&prompt_src, ws_root.join("PROMPT.md"))
            .context("Failed to copy PROMPT.md")?;
    }

    let ralph_src = member_src.join("ralph.yml");
    if ralph_src.exists() {
        fs::copy(&ralph_src, ws_root.join("ralph.yml"))
            .context("Failed to copy ralph.yml")?;
    }

    // Assemble agent dir with symlinks into team/ submodule
    assemble_agent_dir_submodule(ws_root, member_dir_name, project_names, coding_agent)?;

    // Write .botminter.workspace marker
    write_workspace_marker(ws_root, member_dir_name)?;

    // Inject workspace context into context file and .botminter.workspace
    super::context::inject_workspace_context(
        ws_root,
        member_dir_name,
        &coding_agent.context_file,
        github_repo,
        project_number,
        project_names,
    )?;

    Ok(())
}

/// Assembles the agent directory from three scopes using `team/` submodule paths.
///
/// 1. Team-level: `team/coding-agent/agents/*.md`
/// 2. Project-level: `team/projects/{project}/coding-agent/agents/*.md`
/// 3. Member-level: `team/members/{member}/coding-agent/agents/*.md`
///
/// Also copies `settings.local.json` from the member's coding-agent dir if present.
pub(super) fn assemble_agent_dir_submodule(
    ws_root: &Path,
    member_dir_name: &str,
    project_names: &[&str],
    coding_agent: &CodingAgentDef,
) -> Result<()> {
    let agents_subdir = ws_root.join(&coding_agent.agent_dir).join("agents");

    // Remove and recreate for idempotency
    if agents_subdir.exists() {
        fs::remove_dir_all(&agents_subdir).ok();
    }
    fs::create_dir_all(&agents_subdir)
        .with_context(|| format!("Failed to create {}/agents/", coding_agent.agent_dir))?;

    let team_sub = ws_root.join("team");

    // 1. Team-level agents
    symlink_md_files(&team_sub.join("coding-agent").join("agents"), &agents_subdir)?;

    // 2. Project-level agents (all assigned projects)
    for project in project_names {
        symlink_md_files(
            &team_sub
                .join("projects")
                .join(project)
                .join("coding-agent")
                .join("agents"),
            &agents_subdir,
        )?;
    }

    // 3. Member-level agents
    symlink_md_files(
        &team_sub
            .join("members")
            .join(member_dir_name)
            .join("coding-agent")
            .join("agents"),
        &agents_subdir,
    )?;

    // 4. Skills (team → .claude/skills/)
    let skills_subdir = ws_root.join(&coding_agent.agent_dir).join("skills");
    if skills_subdir.exists() {
        fs::remove_dir_all(&skills_subdir).ok();
    }
    fs::create_dir_all(&skills_subdir)
        .with_context(|| format!("Failed to create {}/skills/", coding_agent.agent_dir))?;
    symlink_subdirs(&team_sub.join("coding-agent").join("skills"), &skills_subdir)?;
    for project in project_names {
        symlink_subdirs(
            &team_sub.join("projects").join(project).join("coding-agent").join("skills"),
            &skills_subdir,
        )?;
    }
    symlink_subdirs(
        &team_sub.join("members").join(member_dir_name).join("coding-agent").join("skills"),
        &skills_subdir,
    )?;

    // 5. Commands (team → .claude/commands/)
    let commands_subdir = ws_root.join(&coding_agent.agent_dir).join("commands");
    if commands_subdir.exists() {
        fs::remove_dir_all(&commands_subdir).ok();
    }
    fs::create_dir_all(&commands_subdir)
        .with_context(|| format!("Failed to create {}/commands/", coding_agent.agent_dir))?;
    symlink_subdirs(&team_sub.join("coding-agent").join("commands"), &commands_subdir)?;
    for project in project_names {
        symlink_subdirs(
            &team_sub.join("projects").join(project).join("coding-agent").join("commands"),
            &commands_subdir,
        )?;
    }
    symlink_subdirs(
        &team_sub.join("members").join(member_dir_name).join("coding-agent").join("commands"),
        &commands_subdir,
    )?;

    // 6. Copy settings.local.json if present (member-level)
    let settings_src = team_sub
        .join("members")
        .join(member_dir_name)
        .join("coding-agent")
        .join("settings.local.json");
    if settings_src.exists() {
        let dst = ws_root
            .join(&coding_agent.agent_dir)
            .join("settings.local.json");
        fs::copy(&settings_src, &dst).context("Failed to copy settings.local.json")?;
    }

    // 7. Copy settings.json if present (team-level — shared hooks for all members)
    let team_settings_src = team_sub
        .join("coding-agent")
        .join("settings.json");
    if team_settings_src.exists() {
        let dst = ws_root
            .join(&coding_agent.agent_dir)
            .join("settings.json");
        fs::copy(&team_settings_src, &dst).context("Failed to copy settings.json")?;
    }

    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use super::super::util::{git_cmd_output};
    use std::path::PathBuf;

    /// Returns a `CodingAgentDef` for Claude Code, used by most tests.
    pub fn claude_code_agent() -> CodingAgentDef {
        CodingAgentDef {
            name: "claude-code".into(),
            display_name: "Claude Code".into(),
            context_file: "CLAUDE.md".into(),
            agent_dir: ".claude".into(),
            binary: "claude".into(),
            system_prompt_flag: Some("--append-system-prompt-file".into()),
            skip_permissions_flag: Some("--dangerously-skip-permissions".into()),
        }
    }

    /// Helper: creates a minimal team repo with git, member config, and optional projects.
    pub fn setup_team_repo_for_ws(tmp: &Path) -> PathBuf {
        let team_repo = tmp.join("team_repo");
        let member_cfg = team_repo.join("members/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();
        fs::create_dir_all(member_cfg.join("coding-agent/agents")).unwrap();
        fs::create_dir_all(team_repo.join("coding-agent/agents")).unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["config", "user.email", "test@test"]).unwrap();
        git_cmd(&team_repo, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        team_repo
    }

    /// Helper: creates a fake fork repo for submodule tests.
    pub fn setup_fork_repo(tmp: &Path, name: &str) -> PathBuf {
        let fork = tmp.join(name);
        fs::create_dir_all(&fork).unwrap();
        git_cmd(&fork, &["init", "-b", "main"]).unwrap();
        git_cmd(&fork, &["config", "user.email", "test@test"]).unwrap();
        git_cmd(&fork, &["config", "user.name", "Test"]).unwrap();
        fs::write(fork.join("README.md"), format!("# {}", name)).unwrap();
        git_cmd(&fork, &["add", "-A"]).unwrap();
        git_cmd(&fork, &["commit", "-m", "init fork"]).unwrap();
        fork
    }

    /// Helper: creates workspace repo params for tests (local-only mode).
    pub fn test_ws_params<'a>(
        team_repo: &'a Path,
        workspace_base: &'a Path,
        member: &'a str,
        projects: &'a [(&'a str, &'a str)],
        coding_agent: &'a CodingAgentDef,
    ) -> WorkspaceRepoParams<'a> {
        WorkspaceRepoParams {
            team_repo_path: team_repo,
            workspace_base,
            member_dir_name: member,
            team_name: "my-team",
            projects,
            github_repo: None,
            project_number: None,
            push: false,
            coding_agent,
            remote_ops: None,
            team_submodule_url: None,
        }
    }

    #[test]
    fn workspace_repo_creates_git_repo_with_team_submodule() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");

        // Workspace should be a git repo
        assert!(ws.join(".git").exists(), "workspace should be a git repo");

        // team/ should be a submodule
        assert!(ws.join("team").is_dir(), "team/ submodule should exist");
        assert!(ws.join(".gitmodules").exists(), ".gitmodules should exist");

        // .gitmodules should reference the team submodule
        let gitmodules = fs::read_to_string(ws.join(".gitmodules")).unwrap();
        assert!(
            gitmodules.contains("[submodule \"team\"]"),
            ".gitmodules should contain team submodule entry"
        );
    }

    #[test]
    fn workspace_repo_member_branch_in_team_submodule() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let team_sub = workspace_base.join("arch-01/team");
        let branch = git_cmd_output(&team_sub, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(
            branch.trim(),
            "arch-01",
            "team submodule should be on the member branch"
        );
    }

    #[test]
    fn workspace_repo_with_projects_creates_submodules() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let fork_a = setup_fork_repo(tmp.path(), "project-a");
        let fork_b = setup_fork_repo(tmp.path(), "project-b");

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let fork_a_url = fork_a.to_string_lossy().to_string();
        let fork_b_url = fork_b.to_string_lossy().to_string();
        let projects: Vec<(&str, &str)> = vec![
            ("project-a", &fork_a_url),
            ("project-b", &fork_b_url),
        ];

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &projects, &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");

        // Project submodules should exist
        assert!(
            ws.join("projects/project-a").is_dir(),
            "projects/project-a/ should exist"
        );
        assert!(
            ws.join("projects/project-b").is_dir(),
            "projects/project-b/ should exist"
        );

        // .gitmodules should reference all submodules
        let gitmodules = fs::read_to_string(ws.join(".gitmodules")).unwrap();
        assert!(
            gitmodules.contains("projects/project-a"),
            ".gitmodules should contain project-a"
        );
        assert!(
            gitmodules.contains("projects/project-b"),
            ".gitmodules should contain project-b"
        );

        // Project submodules should contain the fork content
        assert!(
            ws.join("projects/project-a/README.md").exists(),
            "project-a should have README.md from fork"
        );
        assert!(
            ws.join("projects/project-b/README.md").exists(),
            "project-b should have README.md from fork"
        );
    }

    #[test]
    fn workspace_repo_member_branch_in_project_submodules() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let fork = setup_fork_repo(tmp.path(), "my-project");

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let fork_url = fork.to_string_lossy().to_string();
        let projects: Vec<(&str, &str)> = vec![("my-project", &fork_url)];

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &projects, &agent);
        create_workspace_repo(&params).unwrap();

        let proj_sub = workspace_base.join("arch-01/projects/my-project");
        let branch =
            git_cmd_output(&proj_sub, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(
            branch.trim(),
            "arch-01",
            "project submodule should be on the member branch"
        );
    }

    #[test]
    fn workspace_repo_no_projects_has_no_projects_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        assert!(
            !ws.join("projects").exists(),
            "projects/ should not exist when no projects are configured"
        );
    }

    #[test]
    fn workspace_repo_team_submodule_has_member_files() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");

        // The team submodule should contain the team repo content
        assert!(
            ws.join("team/members/arch-01/PROMPT.md").exists(),
            "team submodule should have member PROMPT.md"
        );
        assert!(
            ws.join("team/members/arch-01/CLAUDE.md").exists(),
            "team submodule should have member CLAUDE.md"
        );
        assert!(
            ws.join("team/members/arch-01/ralph.yml").exists(),
            "team submodule should have member ralph.yml"
        );
    }

    // ── Context injection (workspace context) ──────────────────────────

    #[test]
    fn workspace_repo_context_injection_with_params() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());

        // Add botminter.yml to team repo so inject_workspace_context reads it
        let member_cfg = team_repo.join("members/arch-01");
        fs::write(
            member_cfg.join("botminter.yml"),
            "name: Arch One\nrole: architect\n",
        )
        .unwrap();
        git_cmd(&team_repo, &["add", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "add botminter.yml"]).unwrap();

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let mut params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        params.github_repo = Some("testorg/test-team");
        params.project_number = Some(55);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");

        // Verify CLAUDE.md contains injected context with all fields
        let claude_content = fs::read_to_string(ws.join("CLAUDE.md")).unwrap();
        assert!(
            claude_content.contains("<!-- BM:WORKSPACE_CONTEXT -->"),
            "CLAUDE.md should contain context start marker"
        );
        assert!(
            claude_content.contains("| Team repo | `testorg/test-team` |"),
            "CLAUDE.md should contain team repo"
        );
        assert!(
            claude_content.contains("| Project number | `55` |"),
            "CLAUDE.md should contain project number"
        );
        assert!(
            claude_content.contains("| Member | `Arch One` |"),
            "CLAUDE.md should contain member name from botminter.yml"
        );
        assert!(
            claude_content.contains("| Role | `architect` |"),
            "CLAUDE.md should contain role from botminter.yml"
        );

        // Verify .botminter.workspace has KV pairs
        let marker = fs::read_to_string(ws.join(".botminter.workspace")).unwrap();
        assert!(
            marker.contains("team_repo: testorg/test-team"),
            ".botminter.workspace should contain team_repo"
        );
        assert!(
            marker.contains("gh_org: testorg"),
            ".botminter.workspace should contain gh_org"
        );
        assert!(
            marker.contains("project_number: 55"),
            ".botminter.workspace should contain project_number"
        );
    }

    // ── Context assembly (submodule model) ────────────────────────────

    #[test]
    fn workspace_repo_copies_context_files_to_root() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");

        // Context files should exist at workspace root as copies (not symlinks)
        assert!(ws.join("CLAUDE.md").exists(), "CLAUDE.md at workspace root");
        assert!(ws.join("PROMPT.md").exists(), "PROMPT.md at workspace root");
        assert!(ws.join("ralph.yml").exists(), "ralph.yml at workspace root");

        // Verify they are regular files, not symlinks
        assert!(
            !ws.join("CLAUDE.md").symlink_metadata().unwrap().file_type().is_symlink(),
            "CLAUDE.md should be a copy, not a symlink"
        );
        assert!(
            !ws.join("PROMPT.md").symlink_metadata().unwrap().file_type().is_symlink(),
            "PROMPT.md should be a copy, not a symlink"
        );

        // Verify content: CLAUDE.md has original content plus injected workspace context
        let claude_content = fs::read_to_string(ws.join("CLAUDE.md")).unwrap();
        assert!(claude_content.starts_with("# C"), "CLAUDE.md should start with original content");
        assert!(claude_content.contains("<!-- BM:WORKSPACE_CONTEXT -->"), "CLAUDE.md should have injected context");
        assert_eq!(fs::read_to_string(ws.join("PROMPT.md")).unwrap(), "# P");
        assert_eq!(fs::read_to_string(ws.join("ralph.yml")).unwrap(), "v: 1");
    }

    #[test]
    fn workspace_repo_assembles_agent_dir_from_team_submodule() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a team repo with agent files at team and member levels
        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("members/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();

        // Team-level agent
        let team_agents = team_repo.join("coding-agent/agents");
        fs::create_dir_all(&team_agents).unwrap();
        fs::write(team_agents.join("team-agent.md"), "# Team Agent").unwrap();

        // Member-level agent
        let member_agents = member_cfg.join("coding-agent/agents");
        fs::create_dir_all(&member_agents).unwrap();
        fs::write(member_agents.join("member-agent.md"), "# Member Agent").unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["config", "user.email", "test@test"]).unwrap();
        git_cmd(&team_repo, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        let agents_dir = ws.join(".claude/agents");

        // Agent dir should exist
        assert!(agents_dir.is_dir(), ".claude/agents/ should exist");

        // Team-level and member-level agents should be symlinked
        assert!(agents_dir.join("team-agent.md").exists(), "team-agent.md should exist");
        assert!(agents_dir.join("member-agent.md").exists(), "member-agent.md should exist");

        // They should be symlinks
        assert!(
            agents_dir.join("team-agent.md").symlink_metadata().unwrap().file_type().is_symlink(),
            "team-agent.md should be a symlink"
        );
        assert!(
            agents_dir.join("member-agent.md").symlink_metadata().unwrap().file_type().is_symlink(),
            "member-agent.md should be a symlink"
        );

        // Symlinks should resolve to content in team/ submodule
        assert_eq!(
            fs::read_to_string(agents_dir.join("team-agent.md")).unwrap(),
            "# Team Agent"
        );
        assert_eq!(
            fs::read_to_string(agents_dir.join("member-agent.md")).unwrap(),
            "# Member Agent"
        );
    }

    #[test]
    fn workspace_repo_agent_dir_three_scopes() {
        let tmp = tempfile::tempdir().unwrap();

        // Create team repo with agent files at all three scopes
        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("members/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();

        // Team-level
        let team_agents = team_repo.join("coding-agent/agents");
        fs::create_dir_all(&team_agents).unwrap();
        fs::write(team_agents.join("team-wide.md"), "# Team").unwrap();

        // Project-level
        let proj_agents = team_repo.join("projects/myproj/coding-agent/agents");
        fs::create_dir_all(&proj_agents).unwrap();
        fs::write(proj_agents.join("project-specific.md"), "# Project").unwrap();

        // Member-level
        let member_agents = member_cfg.join("coding-agent/agents");
        fs::create_dir_all(&member_agents).unwrap();
        fs::write(member_agents.join("member-only.md"), "# Member").unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["config", "user.email", "test@test"]).unwrap();
        git_cmd(&team_repo, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        // Create a fake project fork
        let fork = setup_fork_repo(tmp.path(), "myproj");
        let fork_url = fork.to_string_lossy().to_string();
        let projects: Vec<(&str, &str)> = vec![("myproj", &fork_url)];

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &projects, &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        let agents_dir = ws.join(".claude/agents");

        // All three scopes should be present
        assert!(agents_dir.join("team-wide.md").exists(), "Team agent missing");
        assert!(agents_dir.join("project-specific.md").exists(), "Project agent missing");
        assert!(agents_dir.join("member-only.md").exists(), "Member agent missing");

        // All should be symlinks
        for name in &["team-wide.md", "project-specific.md", "member-only.md"] {
            assert!(
                agents_dir.join(name).symlink_metadata().unwrap().file_type().is_symlink(),
                "{} should be a symlink",
                name
            );
        }
    }


    #[test]
    fn workspace_repo_writes_marker_file() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        assert!(ws.join(".botminter.workspace").exists(), ".botminter.workspace marker should exist");

        let marker = fs::read_to_string(ws.join(".botminter.workspace")).unwrap();
        assert!(marker.contains("member: arch-01"), "marker should contain member name");
    }

    #[test]
    fn workspace_repo_commits_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");

        // Check that there's a commit
        let log = git_cmd_output(&ws, &["log", "--oneline", "-1"]).unwrap();
        assert!(
            log.contains("Initial workspace setup"),
            "should have initial commit, got: {}",
            log.trim()
        );

        // Working tree should be clean (all files committed)
        let status = git_cmd_output(&ws, &["status", "--porcelain"]).unwrap();
        assert!(
            status.trim().is_empty(),
            "working tree should be clean after commit, got: {}",
            status.trim()
        );
    }

    #[test]
    fn workspace_repo_symlinks_resolve_into_team_submodule() {
        let tmp = tempfile::tempdir().unwrap();

        // Create team repo with a team-level agent
        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("members/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();
        fs::create_dir_all(member_cfg.join("coding-agent/agents")).unwrap();

        let team_agents = team_repo.join("coding-agent/agents");
        fs::create_dir_all(&team_agents).unwrap();
        fs::write(team_agents.join("checker.md"), "# Check").unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["config", "user.email", "test@test"]).unwrap();
        git_cmd(&team_repo, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        let link = ws.join(".claude/agents/checker.md");

        // The symlink target should contain "team/" (pointing into the submodule)
        let target = fs::read_link(&link).unwrap();
        let target_str = target.to_string_lossy();
        assert!(
            target_str.contains("team/"),
            "symlink should point into team/ submodule, got: {}",
            target_str
        );
    }

    // ── MockRemoteOps & push-path tests ──────────────────────────────

    use std::cell::RefCell;

    /// Mock implementation of `RemoteRepoOps` for testing push-path logic.
    ///
    /// Records all calls and simulates state transitions:
    /// - After `create_repo`, state becomes `Empty`.
    /// - After `push_repo`, state becomes `HasContent`.
    ///
    /// `clone_repo` creates a bare git repo at the target path (simulating a
    /// real clone) so that subsequent git operations succeed.
    struct MockRemoteOps {
        _initial_state: RemoteRepoState,
        push_fails: bool,
        calls: RefCell<Vec<String>>,
        state: RefCell<RemoteRepoState>,
    }

    impl MockRemoteOps {
        fn new(initial_state: RemoteRepoState, push_fails: bool) -> Self {
            Self {
                _initial_state: initial_state,
                push_fails,
                calls: RefCell::new(Vec::new()),
                state: RefCell::new(initial_state),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl RemoteRepoOps for MockRemoteOps {
        fn repo_state(&self, _repo_name: &str) -> Result<RemoteRepoState> {
            self.calls.borrow_mut().push("repo_state".into());
            Ok(*self.state.borrow())
        }

        fn create_repo(&self, _repo_name: &str) -> Result<()> {
            self.calls.borrow_mut().push("create_repo".into());
            *self.state.borrow_mut() = RemoteRepoState::Empty;
            Ok(())
        }

        fn delete_repo(&self, _repo_name: &str) -> Result<()> {
            self.calls.borrow_mut().push("delete_repo".into());
            *self.state.borrow_mut() = RemoteRepoState::NotFound;
            Ok(())
        }

        fn clone_repo(&self, _repo_name: &str, target: &Path, recursive: bool) -> Result<()> {
            let tag = if recursive { "clone_repo(recursive)" } else { "clone_repo" };
            self.calls.borrow_mut().push(tag.into());
            // Simulate clone by creating a git repo at the target
            fs::create_dir_all(target)?;
            git_cmd(target, &["init", "-b", "main"])?;
            git_cmd(target, &["config", "user.email", "mock@test"])?;
            git_cmd(target, &["config", "user.name", "Mock"])?;
            Ok(())
        }

        fn push_repo(&self, _local_path: &Path) -> Result<()> {
            self.calls.borrow_mut().push("push_repo".into());
            if self.push_fails {
                bail!("simulated push failure");
            }
            *self.state.borrow_mut() = RemoteRepoState::HasContent;
            Ok(())
        }
    }

    /// Helper: creates push-mode workspace repo params with a mock.
    ///
    /// Uses `team_submodule_url` override to point at the local team repo
    /// (avoids hitting GitHub for submodule add).
    fn push_ws_params<'a>(
        team_repo: &'a Path,
        workspace_base: &'a Path,
        member: &'a str,
        coding_agent: &'a CodingAgentDef,
        remote_ops: &'a dyn RemoteRepoOps,
        team_repo_url: &'a str,
    ) -> WorkspaceRepoParams<'a> {
        WorkspaceRepoParams {
            team_repo_path: team_repo,
            workspace_base,
            member_dir_name: member,
            team_name: "my-team",
            projects: &[],
            github_repo: Some("myorg/my-team"),
            project_number: None,
            push: true,
            coding_agent,
            remote_ops: Some(remote_ops),
            team_submodule_url: Some(team_repo_url),
        }
    }

    #[test]
    fn push_fresh_repo_creates_and_pushes() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let team_url = fs::canonicalize(&team_repo).unwrap().to_string_lossy().to_string();
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let mock = MockRemoteOps::new(RemoteRepoState::NotFound, false);
        let agent = claude_code_agent();
        let params = push_ws_params(&team_repo, &workspace_base, "arch-01", &agent, &mock, &team_url);
        create_workspace_repo(&params).unwrap();

        let calls = mock.calls();
        assert_eq!(
            calls,
            vec!["repo_state", "create_repo", "repo_state", "clone_repo", "push_repo"],
            "fresh repo should: check state, create, wait for availability, clone, push"
        );

        // Workspace should have all context files
        let ws = workspace_base.join("arch-01");
        assert!(ws.join("ralph.yml").exists(), "ralph.yml missing");
        assert!(ws.join("CLAUDE.md").exists(), "CLAUDE.md missing");
        assert!(ws.join("PROMPT.md").exists(), "PROMPT.md missing");
        assert!(ws.join(".botminter.workspace").exists(), "marker missing");
    }

    #[test]
    fn push_repo_has_content_returns_early() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let team_url = fs::canonicalize(&team_repo).unwrap().to_string_lossy().to_string();
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let mock = MockRemoteOps::new(RemoteRepoState::HasContent, false);
        let agent = claude_code_agent();
        let params = push_ws_params(&team_repo, &workspace_base, "arch-01", &agent, &mock, &team_url);
        create_workspace_repo(&params).unwrap();

        let calls = mock.calls();
        assert_eq!(
            calls,
            vec!["repo_state", "clone_repo(recursive)"],
            "HasContent should only check state and clone recursively"
        );

        // Should NOT have called create_repo or push_repo
        assert!(!calls.contains(&"create_repo".to_string()));
        assert!(!calls.contains(&"push_repo".to_string()));
    }

    #[test]
    fn push_empty_repo_deletes_and_recreates() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let team_url = fs::canonicalize(&team_repo).unwrap().to_string_lossy().to_string();
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let mock = MockRemoteOps::new(RemoteRepoState::Empty, false);
        let agent = claude_code_agent();
        let params = push_ws_params(&team_repo, &workspace_base, "arch-01", &agent, &mock, &team_url);
        create_workspace_repo(&params).unwrap();

        let calls = mock.calls();
        assert_eq!(
            calls,
            vec!["repo_state", "delete_repo", "create_repo", "repo_state", "clone_repo", "push_repo"],
            "Empty repo should: check, delete, create, wait for availability, clone, push"
        );

        // Workspace should have all context files
        let ws = workspace_base.join("arch-01");
        assert!(ws.join("ralph.yml").exists(), "ralph.yml missing");
        assert!(ws.join("CLAUDE.md").exists(), "CLAUDE.md missing");
        assert!(ws.join("PROMPT.md").exists(), "PROMPT.md missing");
        assert!(ws.join(".botminter.workspace").exists(), "marker missing");
    }

    #[test]
    fn workspace_repo_surfaces_team_settings_json() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a team repo with coding-agent/settings.json at team level
        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("members/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();
        fs::create_dir_all(member_cfg.join("coding-agent/agents")).unwrap();

        let team_coding_agent = team_repo.join("coding-agent");
        fs::create_dir_all(team_coding_agent.join("agents")).unwrap();
        fs::write(
            team_coding_agent.join("settings.json"),
            r#"{"hooks":{"PostToolUse":[{"hooks":[{"type":"command","command":"bm-agent claude hook post-tool-use"}]}]}}"#,
        ).unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["config", "user.email", "test@test"]).unwrap();
        git_cmd(&team_repo, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        let settings = ws.join(".claude/settings.json");
        assert!(settings.exists(), ".claude/settings.json should be surfaced");
        let content = fs::read_to_string(&settings).unwrap();
        assert!(
            content.contains("bm-agent claude hook post-tool-use"),
            "settings.json should contain PostToolUse hook command"
        );
    }

    #[test]
    fn workspace_repo_no_settings_json_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        assert!(
            !ws.join(".claude/settings.json").exists(),
            ".claude/settings.json should not exist when team has no settings.json"
        );
    }

    #[test]
    fn workspace_repo_settings_json_content_preserved() {
        let tmp = tempfile::tempdir().unwrap();

        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("members/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();
        fs::create_dir_all(member_cfg.join("coding-agent/agents")).unwrap();

        let team_coding_agent = team_repo.join("coding-agent");
        fs::create_dir_all(team_coding_agent.join("agents")).unwrap();
        let original_content = r#"{
  "hooks": {
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "bm-agent claude hook post-tool-use"
          }
        ]
      }
    ]
  }
}
"#;
        fs::write(team_coding_agent.join("settings.json"), original_content).unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["config", "user.email", "test@test"]).unwrap();
        git_cmd(&team_repo, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, "arch-01", &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        let copied = fs::read_to_string(ws.join(".claude/settings.json")).unwrap();
        assert_eq!(copied, original_content, "settings.json content should be byte-for-byte identical");
    }

    #[test]
    fn push_cleans_stale_local_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let team_url = fs::canonicalize(&team_repo).unwrap().to_string_lossy().to_string();
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        // Create a stale local dir with junk
        let stale = workspace_base.join("arch-01");
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("junk.txt"), "leftover").unwrap();

        let mock = MockRemoteOps::new(RemoteRepoState::NotFound, false);
        let agent = claude_code_agent();
        let params = push_ws_params(&team_repo, &workspace_base, "arch-01", &agent, &mock, &team_url);
        create_workspace_repo(&params).unwrap();

        // Junk should be gone, workspace should be fully assembled
        let ws = workspace_base.join("arch-01");
        assert!(!ws.join("junk.txt").exists(), "stale junk should be cleaned up");
        assert!(ws.join("ralph.yml").exists(), "ralph.yml missing after cleanup");
        assert!(ws.join(".botminter.workspace").exists(), "marker missing after cleanup");
    }

    #[test]
    fn push_empty_repo_cleans_stale_local_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = setup_team_repo_for_ws(tmp.path());
        let team_url = fs::canonicalize(&team_repo).unwrap().to_string_lossy().to_string();
        let workspace_base = tmp.path().join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();

        // Create a stale local dir with junk
        let stale = workspace_base.join("arch-01");
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("leftover.txt"), "stale").unwrap();

        let mock = MockRemoteOps::new(RemoteRepoState::Empty, false);
        let agent = claude_code_agent();
        let params = push_ws_params(&team_repo, &workspace_base, "arch-01", &agent, &mock, &team_url);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join("arch-01");
        assert!(!ws.join("leftover.txt").exists(), "stale file should be cleaned up");
        assert!(ws.join("ralph.yml").exists(), "ralph.yml missing after re-create");
        assert!(ws.join("CLAUDE.md").exists(), "CLAUDE.md missing after re-create");
    }
}
