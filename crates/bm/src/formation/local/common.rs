use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::daemon::{self, DaemonClient};
use crate::formation::start_members::{MemberLaunched, MemberSkipped, StartResult};
use crate::formation::stop_members::StopResult;
use crate::formation::{
    self, EnvironmentCheck, EnvironmentStatus, MemberHandle, MemberStatus, StartParams, StopParams,
};
use crate::state;

pub(crate) fn check_environment() -> Result<EnvironmentStatus> {
    let ralph_installed = which::which("ralph").is_ok();
    let just_installed = which::which("just").is_ok();

    let checks = vec![
        EnvironmentCheck {
            name: "ralph".to_string(),
            passed: ralph_installed,
            detail: if ralph_installed {
                "ralph-orchestrator found in PATH".to_string()
            } else {
                "ralph-orchestrator not found in PATH. Install it first.".to_string()
            },
        },
        EnvironmentCheck {
            name: "just".to_string(),
            passed: just_installed,
            detail: if just_installed {
                "just command runner found in PATH".to_string()
            } else {
                "just not found in PATH. Required for bridge lifecycle.".to_string()
            },
        },
    ];

    let ready = checks.iter().all(|c| c.passed);
    Ok(EnvironmentStatus { ready, checks })
}

pub(crate) fn check_prerequisites() -> Result<()> {
    if which::which("ralph").is_err() {
        bail!("'ralph' not found in PATH. Install ralph-orchestrator first.");
    }
    Ok(())
}

pub(crate) fn setup_token_delivery(workspace: &Path, bot_user: &str) -> Result<()> {
    let gh_config_dir = workspace.join(".config").join("gh");
    fs::create_dir_all(&gh_config_dir)
        .with_context(|| format!("Failed to create {}", gh_config_dir.display()))?;

    let hosts_yml = gh_config_dir.join("hosts.yml");
    let hosts_content = format!(
        "github.com:\n    user: {bot_user}\n    oauth_token: placeholder\n    git_protocol: https\n"
    );
    fs::write(&hosts_yml, &hosts_content)
        .with_context(|| format!("Failed to write {}", hosts_yml.display()))?;

    let git_config = workspace.join(".git").join("config");
    if git_config.exists() {
        let existing = fs::read_to_string(&git_config)
            .with_context(|| format!("Failed to read {}", git_config.display()))?;
        let updated = normalize_github_credential_helper(&existing, &resolve_gh_helper_command());
        if updated != existing {
            fs::write(&git_config, updated)
                .with_context(|| format!("Failed to write {}", git_config.display()))?;
        }
    }

    Ok(())
}

pub(crate) fn refresh_token(workspace: &Path, token: &str) -> Result<()> {
    let gh_config_dir = workspace.join(".config").join("gh");
    let hosts_yml = gh_config_dir.join("hosts.yml");

    let existing = fs::read_to_string(&hosts_yml)
        .with_context(|| format!("Failed to read {}", hosts_yml.display()))?;

    let bot_user = existing
        .lines()
        .find_map(|line| line.trim().strip_prefix("user: "))
        .unwrap_or("bot");

    let hosts_content = format!(
        "github.com:\n    user: {bot_user}\n    oauth_token: {token}\n    git_protocol: https\n"
    );

    let tmp_path = gh_config_dir.join("hosts.yml.tmp");
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp_path)
        .with_context(|| format!("Failed to create {}", tmp_path.display()))?
        .write_all(hosts_content.as_bytes())
        .with_context(|| format!("Failed to write {}", tmp_path.display()))?;

    fs::rename(&tmp_path, &hosts_yml).with_context(|| {
        format!(
            "Failed to rename {} → {}",
            tmp_path.display(),
            hosts_yml.display()
        )
    })?;

    Ok(())
}

pub(crate) fn start_members(team_name: &str, params: &StartParams) -> Result<StartResult> {
    check_prerequisites()?;

    let client = match DaemonClient::connect(team_name) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Starting daemon for team '{}'...", team_name);
            daemon::start_daemon(team_name, params.team_repo, "poll", 0, 60, "127.0.0.1")?;
            DaemonClient::connect(team_name)?
        }
    };

    let req = daemon::StartMembersRequest {
        member: params.member_filter.map(|s| s.to_string()),
    };
    let resp = client.start_members(&req)?;

    Ok(StartResult {
        launched: resp
            .launched
            .into_iter()
            .map(|m| MemberLaunched {
                name: m.name,
                pid: m.pid,
                brain_mode: m.brain_mode,
            })
            .collect(),
        skipped: resp
            .skipped
            .into_iter()
            .map(|m| MemberSkipped {
                name: m.name,
                pid: m.pid,
            })
            .collect(),
        errors: resp
            .errors
            .into_iter()
            .map(|m| formation::MemberFailed {
                name: m.name,
                error: m.error,
            })
            .collect(),
        stale_cleaned: vec![],
        bridge: None,
    })
}

pub(crate) fn stop_members(params: &StopParams) -> Result<StopResult> {
    formation::stop_local_members(
        params.team,
        params.config,
        params.member_filter,
        params.force,
        false,
    )
}

pub(crate) fn member_status(team_name: &str) -> Result<Vec<MemberStatus>> {
    let runtime_state = state::load()?;
    let team_prefix = format!("{}/", team_name);

    let statuses = runtime_state
        .members
        .iter()
        .filter(|(key, _)| key.starts_with(&team_prefix))
        .map(|(key, rt)| {
            let member_name = key.strip_prefix(&team_prefix).unwrap_or(key);
            MemberStatus {
                name: member_name.to_string(),
                running: state::is_alive(rt.pid),
                pid: Some(rt.pid),
                workspace: Some(rt.workspace.clone()),
                brain_mode: rt.brain_mode,
            }
        })
        .collect();

    Ok(statuses)
}

pub(crate) fn exec_in(workspace: &Path, cmd: &[&str]) -> Result<()> {
    if cmd.is_empty() {
        bail!("No command specified");
    }

    let status = Command::new(cmd[0])
        .args(&cmd[1..])
        .current_dir(workspace)
        .status()?;

    if !status.success() {
        bail!(
            "Command '{}' exited with status {}",
            cmd.join(" "),
            status.code().unwrap_or(-1)
        );
    }

    Ok(())
}

pub(crate) fn shell() -> Result<()> {
    bail!(
        "shell() is not applicable for local formation — \
         you are already in the local environment"
    )
}

pub(crate) fn write_topology(
    workzone: &Path,
    team_name: &str,
    _members: &[(String, MemberHandle)],
) -> Result<()> {
    let runtime_state = state::load()?;
    formation::write_local_topology(workzone, team_name, &runtime_state)
}

fn gh_helper_command_from_path(path: Option<&Path>) -> String {
    match path {
        Some(path) => format!("!{} auth git-credential", shell_quote(path)),
        None => "!gh auth git-credential".to_string(),
    }
}

fn shell_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\\''"))
}

pub(crate) fn resolve_gh_helper_command() -> String {
    gh_helper_command_from_path(which::which("gh").ok().as_deref())
}

fn github_credential_block(helper: &str) -> String {
    format!("\n[credential \"https://github.com\"]\n\thelper = \n\thelper = {helper}\n")
}

pub(crate) fn normalize_github_credential_helper(existing: &str, helper: &str) -> String {
    let old_helpers = [
        "!/usr/bin/gh auth git-credential",
        "!gh auth git-credential",
    ];

    let mut updated = existing.to_string();
    for old_helper in old_helpers {
        updated = updated.replace(old_helper, helper);
    }

    if updated.contains("[credential \"https://github.com\"]") {
        return updated;
    }

    updated.push_str(&github_credential_block(helper));
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_token_delivery_creates_gh_config_dir_and_hosts_yml() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "[core]\n\tbare = false\n").unwrap();

        setup_token_delivery(tmp.path(), "test-bot[bot]").unwrap();

        let gh_dir = tmp.path().join(".config/gh");
        assert!(gh_dir.exists(), "GH config dir should exist");

        let hosts = std::fs::read_to_string(gh_dir.join("hosts.yml")).unwrap();
        assert!(hosts.contains("user: test-bot[bot]"));
        assert!(hosts.contains("git_protocol: https"));

        let git_config = std::fs::read_to_string(tmp.path().join(".git/config")).unwrap();
        assert!(git_config.contains("[credential \"https://github.com\"]"));
        assert!(git_config.contains("auth git-credential"));
    }

    #[test]
    fn setup_token_delivery_idempotent_credential_helper() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "[core]\n\tbare = false\n").unwrap();

        setup_token_delivery(tmp.path(), "bot").unwrap();
        setup_token_delivery(tmp.path(), "bot").unwrap();

        let git_config = std::fs::read_to_string(tmp.path().join(".git/config")).unwrap();
        let count = git_config
            .matches("[credential \"https://github.com\"]")
            .count();
        assert_eq!(count, 1, "credential helper should be added only once");
    }

    #[test]
    fn setup_token_delivery_rewrites_legacy_usr_bin_gh_helper() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(
            tmp.path().join(".git/config"),
            "[credential \"https://github.com\"]\n\thelper = \n\thelper = !/usr/bin/gh auth git-credential\n",
        )
        .unwrap();

        setup_token_delivery(tmp.path(), "bot").unwrap();

        let git_config = std::fs::read_to_string(tmp.path().join(".git/config")).unwrap();
        assert!(!git_config.contains("!/usr/bin/gh auth git-credential"));
        assert!(git_config.contains("auth git-credential"));
    }

    #[test]
    fn refresh_token_atomically_updates_hosts_yml() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "").unwrap();

        setup_token_delivery(tmp.path(), "my-bot[bot]").unwrap();
        refresh_token(tmp.path(), "ghs_installation_token_abc").unwrap();

        let hosts = std::fs::read_to_string(tmp.path().join(".config/gh/hosts.yml")).unwrap();
        assert!(hosts.contains("oauth_token: ghs_installation_token_abc"));
        assert!(hosts.contains("user: my-bot[bot]"));
        assert!(!hosts.contains("placeholder"));
        assert!(!tmp.path().join(".config/gh/hosts.yml.tmp").exists());
    }

    #[test]
    fn refresh_token_subsequent_updates_replace_token() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "").unwrap();

        setup_token_delivery(tmp.path(), "bot").unwrap();
        refresh_token(tmp.path(), "ghs_first").unwrap();
        refresh_token(tmp.path(), "ghs_second").unwrap();

        let hosts = std::fs::read_to_string(tmp.path().join(".config/gh/hosts.yml")).unwrap();
        assert!(hosts.contains("ghs_second"));
        assert!(!hosts.contains("ghs_first"));
    }

    #[test]
    fn resolve_gh_helper_command_quotes_absolute_path() {
        let helper = gh_helper_command_from_path(Some(Path::new("/opt/homebrew/bin/gh")));
        assert_eq!(helper, "!'/opt/homebrew/bin/gh' auth git-credential");
    }

    #[test]
    fn resolve_gh_helper_command_falls_back_to_plain_gh() {
        let helper = gh_helper_command_from_path(None);
        assert_eq!(helper, "!gh auth git-credential");
    }

    #[test]
    fn check_environment_returns_status() {
        let status = check_environment().unwrap();
        assert_eq!(status.checks.len(), 2);
        assert_eq!(status.checks[0].name, "ralph");
        assert_eq!(status.checks[1].name, "just");
    }

    #[test]
    fn exec_in_empty_cmd_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = exec_in(tmp.path(), &[]).unwrap_err();
        assert!(err.to_string().contains("No command specified"));
    }

    #[test]
    fn exec_in_runs_command() {
        let tmp = tempfile::tempdir().unwrap();
        exec_in(tmp.path(), &["true"]).unwrap();
    }

    #[test]
    fn exec_in_reports_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let err = exec_in(tmp.path(), &["false"]).unwrap_err();
        assert!(err.to_string().contains("exited with status"));
    }

    #[test]
    fn shell_returns_error() {
        let err = shell().unwrap_err();
        assert!(err.to_string().contains("not applicable"));
    }

    #[test]
    fn normalize_github_credential_helper_adds_block_when_missing() {
        let config = "[core]\n\tbare = false\n";
        let updated = normalize_github_credential_helper(config, "!gh auth git-credential");
        assert!(updated.contains("[credential \"https://github.com\"]"));
        assert!(updated.contains("!gh auth git-credential"));
    }
}
