use std::path::Path;

use anyhow::Result;

/// Dirty-state report for a single repository within a session workspace.
#[derive(Debug, Clone)]
pub struct RepoDirtyState {
    pub repo_name: String,
    pub uncommitted_files: Vec<String>,
    pub unpushed_branches: Vec<String>,
}

impl RepoDirtyState {
    pub fn is_clean(&self) -> bool {
        self.uncommitted_files.is_empty() && self.unpushed_branches.is_empty()
    }
}

/// Inspect repositories under `workspace_path` for dirty state.
///
/// Checks `team/` (if it is a git repo) and all git repos under `projects/`.
/// Returns one `RepoDirtyState` entry per directory that is a git repository.
pub fn inspect_dirty_state(workspace_path: &Path) -> Result<Vec<RepoDirtyState>> {
    let mut results = Vec::new();

    // Check the team/ directory first (may be a git submodule or clone).
    let team_dir = workspace_path.join("team");
    if team_dir.join(".git").exists() {
        let uncommitted = inspect_uncommitted(&team_dir)?;
        let unpushed = inspect_unpushed(&team_dir)?;
        results.push(RepoDirtyState {
            repo_name: "team".to_string(),
            uncommitted_files: uncommitted,
            unpushed_branches: unpushed,
        });
    }

    // Check project repositories under projects/.
    let projects_dir = workspace_path.join("projects");
    if !projects_dir.exists() {
        return Ok(results);
    }

    // Projects may be provisioned as nested paths (e.g. org/repo) so we must recurse.
    collect_project_repos(&projects_dir, &projects_dir, &mut results)?;

    Ok(results)
}

/// Recursively collect git repos under `dir`, computing `repo_name` relative to `projects_root`.
///
/// Projects can be nested (e.g. `projects/org/repo`) when the project name includes the org
/// prefix (common with GitHub-style `owner/repo` project names). A flat layout
/// (e.g. `projects/repo`) also works — the repo_name is just `repo` in that case.
fn collect_project_repos(
    dir: &Path,
    projects_root: &Path,
    results: &mut Vec<RepoDirtyState>,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path.join(".git").exists() {
            let repo_name = path
                .strip_prefix(projects_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| entry.file_name().to_string_lossy().to_string());
            let uncommitted = inspect_uncommitted(&path)?;
            let unpushed = inspect_unpushed(&path)?;
            results.push(RepoDirtyState {
                repo_name,
                uncommitted_files: uncommitted,
                unpushed_branches: unpushed,
            });
        } else {
            collect_project_repos(&path, projects_root, results)?;
        }
    }
    Ok(())
}

fn inspect_uncommitted(repo_path: &Path) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["-C", &repo_path.to_string_lossy(), "status", "--porcelain"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

fn inspect_unpushed(repo_path: &Path) -> Result<Vec<String>> {
    let repo_str = repo_path.to_string_lossy();

    let remote_check = std::process::Command::new("git")
        .args(["-C", &repo_str, "remote"])
        .output()?;
    if String::from_utf8_lossy(&remote_check.stdout)
        .trim()
        .is_empty()
    {
        return Ok(vec![]);
    }

    let output = std::process::Command::new("git")
        .args([
            "-C",
            &repo_str,
            "log",
            "--branches",
            "--not",
            "--remotes",
            "--oneline",
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo(dir: &std::path::Path) {
        Command::new("git")
            .args(["init", "-b", "main"])
            .arg(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "config",
                "user.email",
                "test@test.com",
            ])
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "config",
                "user.name",
                "Test",
            ])
            .status()
            .unwrap();
        fs::write(dir.join("README.md"), "# test\n").unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "add", "."])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "commit", "-m", "init"])
            .status()
            .unwrap();
    }

    fn setup_workspace(tmp: &TempDir) -> std::path::PathBuf {
        let ws = tmp.path().join("workspace");
        let projects = ws.join("projects");
        fs::create_dir_all(&projects).unwrap();
        ws
    }

    // AC-5: Dirty State Reported on Deactivation

    #[test]
    fn clean_repo_returns_no_dirty_state() {
        let tmp = TempDir::new().unwrap();
        let ws = setup_workspace(&tmp);
        let repo = ws.join("projects").join("myproject");
        init_repo(&repo);

        let report = inspect_dirty_state(&ws).unwrap();
        assert_eq!(report.len(), 1, "one project repo must be inspected");
        assert!(
            report[0].is_clean(),
            "clean repo must report no dirty state"
        );
    }

    #[test]
    fn uncommitted_files_detected() {
        let tmp = TempDir::new().unwrap();
        let ws = setup_workspace(&tmp);
        let repo = ws.join("projects").join("myproject");
        init_repo(&repo);

        fs::write(repo.join("dirty.txt"), "uncommitted").unwrap();

        let report = inspect_dirty_state(&ws).unwrap();
        assert!(
            !report[0].uncommitted_files.is_empty(),
            "modified/untracked files must be detected"
        );
    }

    #[test]
    fn unpushed_branches_detected() {
        let tmp = TempDir::new().unwrap();

        let bare = tmp.path().join("remote.git");
        Command::new("git")
            .args(["init", "--bare", "-b", "main"])
            .arg(&bare)
            .status()
            .unwrap();

        let ws = setup_workspace(&tmp);
        let repo = ws.join("projects").join("myproject");
        Command::new("git")
            .args([
                "clone",
                bare.to_str().unwrap(),
                repo.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                repo.to_str().unwrap(),
                "config",
                "user.email",
                "test@test.com",
            ])
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                repo.to_str().unwrap(),
                "config",
                "user.name",
                "Test",
            ])
            .status()
            .unwrap();

        fs::write(repo.join("new.txt"), "local only").unwrap();
        Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "add", "."])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "commit", "-m", "local"])
            .status()
            .unwrap();

        let report = inspect_dirty_state(&ws).unwrap();
        assert!(
            !report[0].unpushed_branches.is_empty(),
            "branches with unpushed commits must be detected"
        );
    }

    #[test]
    fn multiple_repos_inspected_independently() {
        let tmp = TempDir::new().unwrap();
        let ws = setup_workspace(&tmp);

        let repo_a = ws.join("projects").join("project-a");
        let repo_b = ws.join("projects").join("project-b");
        init_repo(&repo_a);
        init_repo(&repo_b);

        fs::write(repo_a.join("dirty.txt"), "uncommitted").unwrap();

        let report = inspect_dirty_state(&ws).unwrap();
        assert_eq!(report.len(), 2, "both repos must be inspected");

        let dirty_count = report.iter().filter(|r| !r.is_clean()).count();
        let clean_count = report.iter().filter(|r| r.is_clean()).count();
        assert_eq!(dirty_count, 1, "exactly one repo should be dirty");
        assert_eq!(clean_count, 1, "exactly one repo should be clean");
    }

    #[test]
    fn nested_project_repo_detected_with_relative_name() {
        // Projects provisioned as org/repo (GitHub-style) live two levels deep under projects/.
        let tmp = TempDir::new().unwrap();
        let ws = setup_workspace(&tmp);

        // Simulate projects/my-org/my-repo/
        let repo = ws.join("projects").join("my-org").join("my-repo");
        init_repo(&repo);
        fs::write(repo.join("dirty.txt"), "uncommitted").unwrap();

        let report = inspect_dirty_state(&ws).unwrap();

        assert_eq!(report.len(), 1, "nested project repo must be detected");
        assert_eq!(
            report[0].repo_name, "my-org/my-repo",
            "repo_name must be relative to projects/ root (org/repo format)"
        );
        assert!(
            !report[0].uncommitted_files.is_empty(),
            "uncommitted file in nested repo must be detected"
        );
    }

    #[test]
    fn flat_and_nested_repos_both_detected() {
        let tmp = TempDir::new().unwrap();
        let ws = setup_workspace(&tmp);

        // Flat: projects/flat-repo/
        let flat_repo = ws.join("projects").join("flat-repo");
        init_repo(&flat_repo);

        // Nested: projects/my-org/nested-repo/
        let nested_repo = ws.join("projects").join("my-org").join("nested-repo");
        init_repo(&nested_repo);
        fs::write(nested_repo.join("work.txt"), "change").unwrap();

        let mut report = inspect_dirty_state(&ws).unwrap();
        report.sort_by(|a, b| a.repo_name.cmp(&b.repo_name));

        assert_eq!(report.len(), 2, "both flat and nested repos must be detected");
        assert_eq!(report[0].repo_name, "flat-repo");
        assert_eq!(report[1].repo_name, "my-org/nested-repo");
        assert!(report[0].is_clean(), "flat-repo must be clean");
        assert!(!report[1].is_clean(), "nested-repo must have uncommitted files");
    }
}
