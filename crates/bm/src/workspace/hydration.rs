//! Workspace hydration pipeline — RepoSource trait, GitWorktreeSource, ConfigAssembler,
//! CredentialRelay, and WorkspaceHydrator.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::session::SessionId;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Maps a repo URL to a stable directory name by taking the first 16 hex chars of its SHA-256.
fn url_to_clone_name(url: &str) -> String {
    let hash = Sha256::digest(url.as_bytes());
    hex::encode(&hash[..8])
}

/// Checks whether the shared clone needs a fetch based on the age of `.bm_last_fetch`.
fn needs_fetch(clone_dir: &Path, threshold: Duration) -> bool {
    let marker = clone_dir.join(".bm_last_fetch");
    match fs::metadata(&marker).and_then(|m| m.modified()) {
        Ok(mtime) => mtime.elapsed().unwrap_or(Duration::MAX) > threshold,
        Err(_) => true,
    }
}

/// Touches `.bm_last_fetch` in the clone dir to record the time of the last fetch.
fn touch_fetch_marker(clone_dir: &Path) -> Result<()> {
    fs::write(clone_dir.join(".bm_last_fetch"), "").context("Failed to write fetch marker")
}

/// Read the bare-clone directory that owns the linked worktree at `target`.
/// The worktree's `.git` file contains `gitdir: <bare>/worktrees/<name>`.
fn clone_dir_for_worktree(target: &Path) -> Result<PathBuf> {
    let git_file = target.join(".git");
    let content =
        fs::read_to_string(&git_file).with_context(|| format!("Failed to read {:?}", git_file))?;
    let gitdir_str = content
        .lines()
        .find_map(|l| l.strip_prefix("gitdir: "))
        .ok_or_else(|| anyhow::anyhow!("No gitdir line in {:?}", git_file))?
        .trim();
    let gitdir = PathBuf::from(gitdir_str);
    // Bare repo layout: <clone>/<worktrees>/<name>  → parent.parent() = <clone>
    gitdir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("Cannot derive clone dir from gitdir {:?}", gitdir))
}

// ── RepoSource trait ────────────────────────────────────────────────────────

/// Abstracts repository provisioning so the strategy can be swapped
/// without changing WorkspaceHydrator (e.g., git worktree vs full clone vs container).
pub trait RepoSource: Send + Sync {
    /// Create a writable working tree at `target` containing `repo_url` at `branch`
    /// (or the repo's default branch when `None`). `target` must not exist beforehand.
    fn provision(&self, repo_url: &str, branch: Option<&str>, target: &Path) -> Result<()>;

    /// Remove the working tree at `target` and release any associated resources
    /// (e.g., prune the git worktree reference from the shared clone).
    fn deprovision(&self, target: &Path) -> Result<()>;
}

// ── GitWorktreeSource ───────────────────────────────────────────────────────

/// Implements RepoSource via `git worktree add` from a shared permanent clone.
/// Fetches the shared clone from remote when its last-fetch timestamp is older
/// than `freshness_threshold`.
pub struct GitWorktreeSource {
    /// Directory that holds permanent shared clones, one per repo URL.
    pub clones_dir: PathBuf,
    /// How old a clone's last-fetch can be before a fetch is forced.
    pub freshness_threshold: Duration,
}

impl GitWorktreeSource {
    pub fn new(clones_dir: PathBuf, freshness_threshold: Duration) -> Self {
        Self {
            clones_dir,
            freshness_threshold,
        }
    }
}

impl RepoSource for GitWorktreeSource {
    fn provision(&self, repo_url: &str, branch: Option<&str>, target: &Path) -> Result<()> {
        let clone_dir = self.clones_dir.join(url_to_clone_name(repo_url));

        if !clone_dir.exists() {
            // First use: create a bare clone of the remote.
            fs::create_dir_all(&self.clones_dir).context("Failed to create clones directory")?;
            // On failure, remove the partial clone dir so retries start fresh.
            if let Err(e) = super::util::git_cmd(
                &self.clones_dir,
                &["clone", "--bare", repo_url, clone_dir.to_str().unwrap()],
            ) {
                let _ = fs::remove_dir_all(&clone_dir);
                return Err(e).with_context(|| {
                    format!(
                        "Failed to clone '{}' — check the URL and network connectivity",
                        repo_url
                    )
                });
            }
            touch_fetch_marker(&clone_dir)?;
        } else if needs_fetch(&clone_dir, self.freshness_threshold) {
            // Existing clone is stale: fetch all refs.
            super::util::git_cmd(&clone_dir, &["fetch", "--all", "--prune"]).with_context(
                || {
                    format!(
                        "git fetch failed on shared clone at '{}' — check network connectivity",
                        clone_dir.display()
                    )
                },
            )?;
            touch_fetch_marker(&clone_dir)?;
        }

        // Resolve the ref to check out (detached HEAD to avoid branch-lock conflicts).
        let commit_ish = match branch {
            Some(b) => b.to_string(),
            None => super::util::git_cmd_output(&clone_dir, &["rev-parse", "HEAD"])
                .ok()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "HEAD".to_string()),
        };

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).context("Failed to create worktree parent dir")?;
        }

        super::util::git_cmd(
            &clone_dir,
            &[
                "worktree",
                "add",
                "--detach",
                target.to_str().unwrap(),
                &commit_ish,
            ],
        )
        .with_context(|| format!("git worktree add failed for '{}'", target.display()))
    }

    fn deprovision(&self, target: &Path) -> Result<()> {
        if !target.exists() {
            return Ok(());
        }
        // Find the owning bare clone, remove the worktree entry, then prune.
        let clone_dir =
            clone_dir_for_worktree(target).context("Failed to find owning clone for worktree")?;

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", target.to_str().unwrap()])
            .current_dir(&clone_dir)
            .output();

        // Ensure the directory is gone regardless.
        if target.exists() {
            fs::remove_dir_all(target)
                .with_context(|| format!("Failed to remove worktree dir {:?}", target))?;
        }

        // Prune stale entries from the shared clone.
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&clone_dir)
            .output();

        Ok(())
    }
}

// ── AssemblyConfig ──────────────────────────────────────────────────────────

/// Per-session parameters that ConfigAssembler injects into the workspace.
#[derive(Debug, Clone)]
pub struct AssemblyConfig {
    pub session_id: SessionId,
    pub member_name: String,
    pub team_repo_url: String,
    pub team_repo_branch: String,
    pub project_number: Option<u64>,
    /// Skill directories to symlink or reference in the assembled workspace.
    pub skill_dirs: Vec<PathBuf>,
    /// Root directory under which per-member credential directories live.
    pub credential_base: PathBuf,
    /// Project names within the team repo (e.g. "botminter") whose coding-agent
    /// assets (agents, skills) are merged into the assembled .claude/ directory.
    pub project_names: Vec<String>,
    /// Team workspace root (e.g. `~/.botminter/workspaces/my-team`). Durable
    /// member state lives at `<workspace_base>/member-state/<member>/`.
    pub workspace_base: PathBuf,
}

impl AssemblyConfig {
    /// Path to the durable member-state directory: `<workspace_base>/member-state/<member>`.
    pub fn member_state_dir(&self) -> PathBuf {
        self.workspace_base.join("member-state").join(&self.member_name)
    }
}

// ── ConfigAssembler ─────────────────────────────────────────────────────────

/// Merge each source directory (if it exists) into `dst` using `merge`.
/// Creates `dst` lazily — only if at least one source is a directory.
fn merge_sources_into(
    sources: impl IntoIterator<Item = PathBuf>,
    dst: &Path,
    merge: fn(&Path, &Path) -> Result<()>,
) -> Result<()> {
    for src in sources {
        if src.is_dir() {
            fs::create_dir_all(dst)
                .with_context(|| format!("Failed to create {}", dst.display()))?;
            merge(&src, dst)?;
        }
    }
    Ok(())
}

/// Populates a session workspace with CLAUDE.md, PROMPT.md, ralph.yml,
/// .claude/agents/ references, .botminter.workspace marker, and skill directory
/// references. All operations are idempotent — running twice yields the same state.
pub struct ConfigAssembler {
    pub team_repo_path: PathBuf,
    pub member_name: String,
}

impl ConfigAssembler {
    pub fn new(team_repo_path: PathBuf, member_name: String) -> Self {
        Self {
            team_repo_path,
            member_name,
        }
    }

    /// Assemble workspace configuration from `config` into `workspace`.
    /// Surfaces PROMPT.md, CLAUDE.md, and ralph.yml from the team repo member
    /// directory, writes the .botminter.workspace marker, and validates skill dirs.
    /// Returns a list of warning strings (e.g., missing files, missing skill dirs).
    pub fn assemble(&self, workspace: &Path, config: &AssemblyConfig) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Surface member config files from team repo (required for ralph to launch).
        // team_repo_path/members/<member>/{PROMPT.md,CLAUDE.md,ralph.yml} → workspace/
        if self.team_repo_path.exists() {
            let member_dir = self
                .team_repo_path
                .join("members")
                .join(&config.member_name);

            // PROMPT.md — required; warn if missing
            let prompt_src = member_dir.join("PROMPT.md");
            if prompt_src.exists() {
                fs::copy(&prompt_src, workspace.join("PROMPT.md")).with_context(|| {
                    format!(
                        "Failed to copy PROMPT.md from {}",
                        prompt_src.display()
                    )
                })?;
            } else {
                warnings.push(format!(
                    "PROMPT.md not found for member '{}' at {} — ralph may not start correctly",
                    config.member_name,
                    prompt_src.display()
                ));
            }

            // CLAUDE.md — optional
            let claude_src = member_dir.join("CLAUDE.md");
            if claude_src.exists() {
                fs::copy(&claude_src, workspace.join("CLAUDE.md")).with_context(|| {
                    format!("Failed to copy CLAUDE.md from {}", claude_src.display())
                })?;
            }

            // ralph.yml — optional but expected for loop sessions
            let ralph_src = member_dir.join("ralph.yml");
            if ralph_src.exists() {
                fs::copy(&ralph_src, workspace.join("ralph.yml")).with_context(|| {
                    format!("Failed to copy ralph.yml from {}", ralph_src.display())
                })?;
            }

            // brain-prompt.md — optional; presence marks member as brain in ephemeral model
            let brain_src = member_dir.join("brain-prompt.md");
            if brain_src.exists() {
                fs::copy(&brain_src, workspace.join("brain-prompt.md")).with_context(|| {
                    format!("Failed to copy brain-prompt.md from {}", brain_src.display())
                })?;
            }
        }

        // dm-room.json — durable member state; persists discovered Matrix DM room across restarts
        let dm_room_src = config.member_state_dir().join("dm-room.json");
        if dm_room_src.exists() {
            fs::copy(&dm_room_src, workspace.join("dm-room.json")).with_context(|| {
                format!("Failed to copy dm-room.json from {}", dm_room_src.display())
            })?;
        }

        // Write .botminter.workspace marker (idempotent: always overwrite).
        let marker = format!(
            "# BotMinter workspace marker — do not delete\nsession_id: {}\nmember: {}\n",
            config.session_id.as_str(),
            config.member_name,
        );
        fs::write(workspace.join(".botminter.workspace"), &marker)
            .context("Failed to write .botminter.workspace marker")?;

        // Validate skill directories and collect warnings for missing ones.
        for dir in &config.skill_dirs {
            if !dir.exists() {
                warnings.push(format!("Skill directory not found: {}", dir.display()));
            }
        }

        // Assemble .claude/ directory from team coding-agent assets (non-fatal).
        if let Err(e) = self.assemble_claude_dir(workspace, config) {
            warnings.push(format!(
                ".claude/ assembly failed: {e} — coding-agent assets (agents, skills, settings) may be missing"
            ));
        }

        Ok(warnings)
    }

    /// Build `<team-repo>/projects/<p>/coding-agent/<subdir>` for each project name.
    fn project_ca_dirs(&self, project_names: &[String], subdir: &str) -> Vec<PathBuf> {
        project_names
            .iter()
            .map(|p| {
                self.team_repo_path
                    .join("projects")
                    .join(p)
                    .join("coding-agent")
                    .join(subdir)
            })
            .collect()
    }

    fn assemble_claude_dir(&self, workspace: &Path, config: &AssemblyConfig) -> Result<()> {
        let claude_dir = workspace.join(".claude");
        fs::create_dir_all(&claude_dir).context("Failed to create .claude/")?;

        let team_ca = self.team_repo_path.join("coding-agent");
        let member_ca = self
            .team_repo_path
            .join("members")
            .join(&self.member_name)
            .join("coding-agent");

        // Agents: team + project-level.
        merge_sources_into(
            std::iter::once(team_ca.join("agents"))
                .chain(self.project_ca_dirs(&config.project_names, "agents")),
            &claude_dir.join("agents"),
            super::util::symlink_md_files,
        )?;

        // Skills: team + member + project-level.
        merge_sources_into(
            [team_ca.join("skills"), member_ca.join("skills")]
                .into_iter()
                .chain(self.project_ca_dirs(&config.project_names, "skills")),
            &claude_dir.join("skills"),
            super::util::symlink_subdirs,
        )?;

        // Commands: member-level only.
        merge_sources_into(
            std::iter::once(member_ca.join("commands")),
            &claude_dir.join("commands"),
            super::util::symlink_md_files,
        )?;

        // Settings: team-level settings.json, member-level settings.local.json.
        let settings_src = team_ca.join("settings.json");
        if settings_src.exists() {
            fs::copy(&settings_src, claude_dir.join("settings.json"))
                .context("Failed to copy settings.json")?;
        }

        let settings_local_src = member_ca.join("settings.local.json");
        if settings_local_src.exists() {
            fs::copy(&settings_local_src, claude_dir.join("settings.local.json"))
                .context("Failed to copy settings.local.json")?;
        }

        Ok(())
    }
}

// ── CredentialRelay ─────────────────────────────────────────────────────────

/// Resolves a GitHub App installation token for a member.
/// Injected into [`HydrationWorkspaceConfig`] so tests can mock keyring access.
pub trait AppTokenProvider: Send + Sync + std::fmt::Debug {
    fn resolve_token(&self, member_name: &str) -> Result<Option<String>>;
}

/// Writes credential files (hosts.yml) into a member's shared credential directory.
pub trait CredentialWriter: Send + Sync {
    fn write_credentials(&self, member_dir: &Path) -> Result<()>;
}

/// A no-op credential writer — used when no App credentials are configured.
pub struct NoOpCredentialWriter;

impl CredentialWriter for NoOpCredentialWriter {
    fn write_credentials(&self, _member_dir: &Path) -> Result<()> {
        Ok(())
    }
}

fn hosts_yml_content(token: &str) -> String {
    format!("github.com:\n    oauth_token: {token}\n    git_protocol: https\n")
}

/// Resolves a GitHub App token via an [`AppTokenProvider`] and writes `hosts.yml`
/// to the member credential directory.
pub struct AppCredentialWriter {
    pub provider: std::sync::Arc<dyn AppTokenProvider>,
}

impl CredentialWriter for AppCredentialWriter {
    fn write_credentials(&self, member_dir: &Path) -> Result<()> {
        // member_dir is <credentials_base>/<member_name>; last component is the member name.
        let member_name = member_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Cannot derive member name from path {:?}", member_dir))?;

        match self.provider.resolve_token(member_name)? {
            Some(token) => {
                let gh_dir = member_dir.join("gh");
                fs::create_dir_all(&gh_dir).with_context(|| {
                    format!("Failed to create credential gh dir {:?}", gh_dir)
                })?;
                let hosts_yml = gh_dir.join("hosts.yml");
                let tmp_path = gh_dir.join(".hosts.yml.tmp");
                fs::write(&tmp_path, hosts_yml_content(&token))
                    .with_context(|| format!("Failed to write temp hosts.yml to {:?}", tmp_path))?;
                fs::rename(&tmp_path, &hosts_yml)
                    .with_context(|| format!("Failed to atomically replace hosts.yml at {:?}", hosts_yml))?;
            }
            None => {
                tracing::warn!(
                    "No token resolved for member '{}' — skipping hosts.yml write",
                    member_name
                );
            }
        }
        Ok(())
    }
}

/// Production AppTokenProvider backed by a KeyValueCredentialStore.
/// Reads client_id, private_key, and installation_id from the store and
/// exchanges them for a GitHub App installation token.
pub struct KeyringAppTokenProvider {
    store: std::sync::Arc<dyn crate::formation::KeyValueCredentialStore + Send + Sync>,
}

impl std::fmt::Debug for KeyringAppTokenProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyringAppTokenProvider").finish()
    }
}

impl KeyringAppTokenProvider {
    pub fn new(
        store: std::sync::Arc<dyn crate::formation::KeyValueCredentialStore + Send + Sync>,
    ) -> Self {
        Self { store }
    }
}

impl AppTokenProvider for KeyringAppTokenProvider {
    fn resolve_token(&self, member_name: &str) -> Result<Option<String>> {
        use crate::git::manifest_flow::credential_keys;

        let client_id = match self.store.retrieve(&credential_keys::client_id(member_name))? {
            Some(v) => v,
            None => return Ok(None),
        };
        let private_key = match self.store.retrieve(&credential_keys::private_key(member_name))? {
            Some(v) => v,
            None => return Ok(None),
        };
        let installation_id_str =
            match self.store.retrieve(&credential_keys::installation_id(member_name))? {
                Some(v) => v,
                None => return Ok(None),
            };
        let installation_id: u64 = installation_id_str
            .parse()
            .context("Invalid installation ID in credential store")?;

        let token = exchange_token(&client_id, &private_key, installation_id)?;
        Ok(Some(token))
    }
}

#[cfg(not(test))]
fn exchange_token(client_id: &str, private_key: &str, installation_id: u64) -> Result<String> {
    use crate::git::app_auth;
    let jwt = app_auth::generate_jwt(client_id, private_key)
        .context("Failed to generate JWT for App authentication")?;
    let inst_token = app_auth::exchange_for_installation_token(&jwt, installation_id)
        .context("Failed to exchange JWT for installation token")?;
    Ok(inst_token.token)
}

#[cfg(test)]
fn exchange_token(_client_id: &str, _private_key: &str, _installation_id: u64) -> Result<String> {
    Ok("ghs_test_token_for_unit_tests".to_string())
}

/// Manages per-member credential directories: writes credential files during
/// session creation and provides the directory path for runtime use.
/// Credentials are never copied into session workspaces — only referenced.
pub struct CredentialRelay {
    /// Root directory under which `<member>/` subdirectories hold credentials.
    pub credentials_base: PathBuf,
    writer: Box<dyn CredentialWriter>,
}

impl CredentialRelay {
    pub fn new(credentials_base: PathBuf) -> Self {
        Self {
            credentials_base,
            writer: Box::new(NoOpCredentialWriter),
        }
    }

    pub fn with_writer(credentials_base: PathBuf, writer: Box<dyn CredentialWriter>) -> Self {
        Self {
            credentials_base,
            writer,
        }
    }

    fn member_dir(&self, member_name: &str) -> PathBuf {
        self.credentials_base.join(member_name)
    }

    /// Return the credential directory for `member_name`. Errors if the member
    /// has no configured credential directory.
    pub fn credential_path(&self, member_name: &str) -> Result<PathBuf> {
        let path = self.member_dir(member_name);
        if !path.exists() {
            bail!(
                "No credential directory for member '{}' at {} \
                 — run 'bm credentials export' or verify member configuration",
                member_name,
                path.display()
            );
        }
        Ok(path)
    }

    /// Write credentials (hosts.yml) to the shared credential directory for `member_name`.
    pub fn ensure_credentials(&self, member_name: &str) -> Result<()> {
        self.writer.write_credentials(&self.member_dir(member_name))
    }

    /// Return `<credentials_base>/<member>/gh` if a valid `hosts.yml` exists there.
    pub fn gh_dir_for(&self, member_name: &str) -> Option<PathBuf> {
        let gh_dir = self.member_dir(member_name).join("gh");
        if gh_dir.join("hosts.yml").exists() {
            Some(gh_dir)
        } else {
            None
        }
    }
}

// ── HydrationTiming ─────────────────────────────────────────────────────────

/// Timing breakdown returned by a successful hydration so operators can
/// identify bottlenecks (clone fetch latency vs worktree creation vs config assembly).
#[derive(Debug, Clone)]
pub struct HydrationTiming {
    /// Time spent in clone/fetch operations inside `RepoSource::provision()`.
    /// Currently always 0 because fetch and worktree creation are not timed
    /// separately inside `provision()` — this field is reserved for future
    /// lower-level instrumentation.
    pub clone_fetch_ms: u64,
    /// Total time spent in `RepoSource::provision()` across all repos,
    /// including any clone fetch and worktree creation.
    pub worktree_create_ms: u64,
    /// Time spent assembling workspace config (marker file, skill validation).
    pub config_assembly_ms: u64,
}

// ── HydrationResult ─────────────────────────────────────────────────────────

/// Returned by WorkspaceHydrator::hydrate on success.
#[derive(Debug)]
pub struct HydrationResult {
    pub workspace_path: PathBuf,
    pub timing: HydrationTiming,
    /// Skill directories that were not found — reported as warnings, not errors.
    pub skill_warnings: Vec<String>,
}

// ── WorkspaceHydrator ───────────────────────────────────────────────────────

/// Orchestrates repo provisioning and config assembly into a complete session workspace.
/// Atomic from the caller's perspective: either a fully valid workspace is returned,
/// or no workspace exists (all partial state is cleaned up on failure).
pub struct WorkspaceHydrator<R: RepoSource> {
    pub repo_source: R,
    pub config_assembler: ConfigAssembler,
    pub credential_relay: CredentialRelay,
    /// Root under which per-member/per-session workspace directories are created.
    pub sessions_base: PathBuf,
}

impl<R: RepoSource> WorkspaceHydrator<R> {
    pub fn new(
        repo_source: R,
        config_assembler: ConfigAssembler,
        credential_relay: CredentialRelay,
        sessions_base: PathBuf,
    ) -> Self {
        Self {
            repo_source,
            config_assembler,
            credential_relay,
            sessions_base,
        }
    }

    /// Create a complete session workspace for `member`.
    /// `repo_urls` is a slice of `(url, project_name)` pairs — one working tree
    /// per entry is created under `workspace/projects/<project_name>/`.
    ///
    /// On any error, all partial state is removed and an error is returned.
    pub fn hydrate(
        &self,
        session_id: &SessionId,
        member: &str,
        repo_urls: &[(&str, &str)],
        config: AssemblyConfig,
    ) -> Result<HydrationResult> {
        let workspace_path = self.sessions_base.join(member).join(session_id.as_str());
        fs::create_dir_all(&workspace_path).with_context(|| {
            format!(
                "Failed to create workspace directory at {} — check available disk space",
                workspace_path.display()
            )
        })?;

        let projects_dir = workspace_path.join("projects");
        if let Err(e) = fs::create_dir_all(&projects_dir) {
            let _ = fs::remove_dir_all(&workspace_path);
            return Err(e).with_context(|| {
                format!(
                    "Failed to create projects directory at {} — check available disk space",
                    projects_dir.display()
                )
            });
        }

        // Provision each repo as a git worktree.  Track what was created for rollback.
        let provision_start = Instant::now();
        let mut provisioned: Vec<PathBuf> = Vec::new();
        for (url, project_name) in repo_urls {
            let project_path = projects_dir.join(project_name);
            if let Err(e) = self.repo_source.provision(url, None, &project_path) {
                for p in &provisioned {
                    let _ = self.repo_source.deprovision(p);
                }
                let _ = fs::remove_dir_all(&workspace_path);
                return Err(e).with_context(|| {
                    format!(
                        "Failed to provision repo '{}' (project '{}')",
                        url, project_name
                    )
                });
            }
            provisioned.push(project_path);
        }
        let worktree_create_ms = provision_start.elapsed().as_millis() as u64;

        // Assemble config (writes marker, validates skills).
        let assembly_start = Instant::now();
        let skill_warnings = match self.config_assembler.assemble(&workspace_path, &config) {
            Ok(w) => w,
            Err(e) => {
                for p in &provisioned {
                    let _ = self.repo_source.deprovision(p);
                }
                let _ = fs::remove_dir_all(&workspace_path);
                return Err(e).with_context(|| {
                    format!(
                        "Failed to assemble workspace config for session {} \
                         — verify team repo path and member configuration",
                        session_id.as_str()
                    )
                });
            }
        };
        let config_assembly_ms = assembly_start.elapsed().as_millis() as u64;

        // Write credentials to the shared dir — non-fatal; session starts without them on failure.
        if let Err(e) = self.credential_relay.ensure_credentials(member) {
            tracing::warn!("Credential write failed for member '{}': {e}", member);
        }

        Ok(HydrationResult {
            workspace_path,
            timing: HydrationTiming {
                clone_fetch_ms: 0,
                worktree_create_ms,
                config_assembly_ms,
            },
            skill_warnings,
        })
    }

    /// Remove the session workspace and all associated git worktrees.
    pub fn teardown(&self, session_id: &SessionId, member: &str) -> Result<()> {
        let workspace_path = self.sessions_base.join(member).join(session_id.as_str());
        if !workspace_path.exists() {
            return Ok(());
        }

        let projects_dir = workspace_path.join("projects");
        if projects_dir.is_dir() {
            for entry in fs::read_dir(&projects_dir).with_context(|| {
                format!(
                    "Failed to list projects directory at {} during teardown",
                    projects_dir.display()
                )
            })? {
                let path = entry?.path();
                if path.is_dir() {
                    let _ = self.repo_source.deprovision(&path);
                }
            }
        }

        fs::remove_dir_all(&workspace_path).with_context(|| {
            format!(
                "Failed to remove workspace at {} — check that no processes hold open files",
                workspace_path.display()
            )
        })
    }
}

// ── Production WorkspaceOps ────────────────────────────────────────────────────

/// Configuration for constructing a [`HydrationWorkspaceOps`].
#[derive(Debug, Clone)]
pub struct HydrationWorkspaceConfig {
    pub clones_dir: PathBuf,
    pub sessions_base: PathBuf,
    pub team_repo_path: PathBuf,
    pub credential_base: PathBuf,
    pub freshness_threshold: Duration,
    pub repo_urls: Vec<(String, String)>,
    pub team_repo_url: String,
    pub team_repo_branch: String,
    /// Team workspace root (e.g. `~/workspaces/my-team`). Permanent member
    /// workspaces live at `<workspace_base>/<member>/` — used to locate App
    /// credential directories (`GH_CONFIG_DIR`) when launching ralph.
    pub workspace_base: PathBuf,
    pub project_number: Option<u64>,
    pub skill_dirs: Vec<PathBuf>,
    /// Optional token provider — when set, `HydrationWorkspaceOps` MUST use it to resolve
    /// a GitHub App token and write `hosts.yml` to `<credential_base>/<member>/`.
    pub credential_resolver: Option<std::sync::Arc<dyn AppTokenProvider>>,
    /// Project names within the team repo (e.g. "botminter") whose coding-agent
    /// assets (agents, skills) are merged into the assembled .claude/ directory.
    pub project_names: Vec<String>,
}

/// Production implementation of [`crate::session::manager::WorkspaceOps`]
/// that delegates to the hydration pipeline for workspace creation and
/// to [`crate::session::dirty_state`] for workspace inspection.
pub struct HydrationWorkspaceOps {
    hydrator: WorkspaceHydrator<GitWorktreeSource>,
    repo_urls: Vec<(String, String)>,
    team_repo_url: String,
    team_repo_branch: String,
    project_number: Option<u64>,
    skill_dirs: Vec<PathBuf>,
    project_names: Vec<String>,
    workspace_base: PathBuf,
}

impl HydrationWorkspaceOps {
    pub fn new(config: HydrationWorkspaceConfig) -> Self {
        let source = GitWorktreeSource::new(config.clones_dir, config.freshness_threshold);
        let assembler = ConfigAssembler::new(config.team_repo_path, String::new());
        let relay = if let Some(provider) = config.credential_resolver {
            CredentialRelay::with_writer(
                config.credential_base,
                Box::new(AppCredentialWriter { provider }),
            )
        } else {
            CredentialRelay::new(config.credential_base)
        };
        let hydrator = WorkspaceHydrator::new(source, assembler, relay, config.sessions_base);

        Self {
            hydrator,
            repo_urls: config.repo_urls,
            team_repo_url: config.team_repo_url,
            team_repo_branch: config.team_repo_branch,
            project_number: config.project_number,
            skill_dirs: config.skill_dirs,
            project_names: config.project_names,
            workspace_base: config.workspace_base,
        }
    }

    pub fn teardown(&self, session_id: &SessionId, member: &str) -> Result<()> {
        self.hydrator.teardown(session_id, member)
    }

    /// Return the team repo path: `<workspace_base>/team`.
    pub fn team_repo_path(&self) -> PathBuf {
        self.workspace_base.join("team")
    }

    /// Return the team repo URL (remote URL, e.g. `https://github.com/org/repo.git`).
    pub fn team_repo_url(&self) -> &str {
        &self.team_repo_url
    }

    /// Return the durable member-state directory: `<workspace_base>/member-state/<member>`.
    pub fn member_state_dir_for_member(&self, member_name: &str) -> PathBuf {
        self.workspace_base.join("member-state").join(member_name)
    }

    /// Return the GitHub App credential directory for `member_name` if it exists.
    ///
    /// The path is `<credential_base>/<member>/gh` (D-02 shared credential path) —
    /// set as `GH_CONFIG_DIR` when launching ralph so it uses the member's App token.
    pub fn gh_config_dir_for_member(&self, member_name: &str) -> Option<PathBuf> {
        self.hydrator.credential_relay.gh_dir_for(member_name)
    }

    /// Refresh credentials for `member_name` by re-writing the credential directory.
    pub fn ensure_credentials(&self, member_name: &str) -> anyhow::Result<()> {
        self.hydrator.credential_relay.ensure_credentials(member_name)
    }
}

impl crate::session::manager::WorkspaceOps for HydrationWorkspaceOps {
    fn hydrate_workspace(&self, session_id: &SessionId, member: &str) -> Result<PathBuf> {
        let config = AssemblyConfig {
            session_id: session_id.clone(),
            member_name: member.to_string(),
            team_repo_url: self.team_repo_url.clone(),
            team_repo_branch: self.team_repo_branch.clone(),
            project_number: self.project_number,
            skill_dirs: self.skill_dirs.clone(),
            credential_base: self.hydrator.credential_relay.credentials_base.clone(),
            project_names: self.project_names.clone(),
            workspace_base: self.workspace_base.clone(),
        };

        let refs: Vec<(&str, &str)> = self
            .repo_urls
            .iter()
            .map(|(url, name)| (url.as_str(), name.as_str()))
            .collect();

        let result = self.hydrator.hydrate(session_id, member, &refs, config)?;
        Ok(result.workspace_path)
    }

    fn inspect_dirty_state(
        &self,
        workspace_path: &Path,
    ) -> Result<Vec<crate::session::dirty_state::RepoDirtyState>> {
        crate::session::dirty_state::inspect_dirty_state(workspace_path)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formation::KeyValueCredentialStore;
    use crate::session::manager::WorkspaceOps;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a bare git repo with one commit. Returns the bare repo path.
    fn init_bare_repo(tmp: &TempDir, name: &str) -> PathBuf {
        let bare = tmp.path().join(format!("{name}.git"));
        fs::create_dir_all(&bare).unwrap();
        Command::new("git")
            .args(["init", "--bare", "-b", "main"])
            .arg(&bare)
            .status()
            .unwrap();

        let work = tmp.path().join(format!("{name}-work"));
        Command::new("git")
            .args(["clone", bare.to_str().unwrap(), work.to_str().unwrap()])
            .status()
            .unwrap();
        fs::write(work.join("README.md"), format!("# {name}\n")).unwrap();
        Command::new("git")
            .args(["-C", work.to_str().unwrap(), "add", "."])
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                work.to_str().unwrap(),
                "-c",
                "user.email=test@test.com",
                "-c",
                "user.name=Test",
                "commit",
                "-m",
                "init",
            ])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", work.to_str().unwrap(), "push"])
            .status()
            .unwrap();
        bare
    }

    fn git_worktree_source(tmp: &TempDir) -> GitWorktreeSource {
        GitWorktreeSource::new(tmp.path().join("clones"), Duration::from_secs(300))
    }

    fn make_assembly_config(
        tmp: &TempDir,
        session_id: SessionId,
        repo_url: &str,
    ) -> AssemblyConfig {
        AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: repo_url.to_string(),
            team_repo_branch: "main".to_string(),
            project_number: Some(42),
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        }
    }

    // ── AC-1: Clean Workspace State ─────────────────────────────────────────

    #[test]
    fn provision_produces_clean_git_working_tree() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        let source = git_worktree_source(&tmp);
        let target = tmp.path().join("ws");

        // provision must succeed and produce a workspace with no uncommitted changes
        source
            .provision(repo.to_str().unwrap(), None, &target)
            .unwrap();

        let status = Command::new("git")
            .args(["-C", target.to_str().unwrap(), "status", "--porcelain"])
            .output()
            .unwrap();
        assert!(
            status.stdout.is_empty(),
            "provisioned workspace must have no uncommitted changes"
        );
    }

    // ── AC-2: Independent Workspaces ────────────────────────────────────────

    #[test]
    fn two_provisions_of_same_repo_yield_independent_working_trees() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        let source = git_worktree_source(&tmp);

        let ws_a = tmp.path().join("ws-a");
        let ws_b = tmp.path().join("ws-b");

        source
            .provision(repo.to_str().unwrap(), None, &ws_a)
            .unwrap();
        source
            .provision(repo.to_str().unwrap(), None, &ws_b)
            .unwrap();

        // Writing to workspace A must not affect workspace B
        fs::write(ws_a.join("mutation.txt"), "only in A").unwrap();
        assert!(
            !ws_b.join("mutation.txt").exists(),
            "mutation in workspace A must not appear in workspace B"
        );
        assert_ne!(ws_a, ws_b, "workspaces must be distinct directories");
    }

    // ── AC-3: Timing Breakdown ──────────────────────────────────────────────

    #[test]
    fn hydrate_returns_timing_breakdown() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        let source = git_worktree_source(&tmp);
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let relay = CredentialRelay::new(tmp.path().join("credentials"));
        let hydrator =
            WorkspaceHydrator::new(source, assembler, relay, tmp.path().join("sessions"));

        let session_id = SessionId::new();
        let config = make_assembly_config(&tmp, session_id.clone(), repo.to_str().unwrap());

        let result = hydrator
            .hydrate(
                &session_id,
                "alice",
                &[(repo.to_str().unwrap(), "myproject")],
                config,
            )
            .unwrap();

        // AC-3: all three timing fields must be present (non-panic access is the assertion)
        let _ = result.timing.clone_fetch_ms;
        let _ = result.timing.worktree_create_ms;
        let _ = result.timing.config_assembly_ms;
    }

    // ── AC-4: Fail-Clean ────────────────────────────────────────────────────

    #[test]
    fn failed_hydration_leaves_no_partial_workspace_state() {
        let tmp = TempDir::new().unwrap();
        let source = git_worktree_source(&tmp);
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let relay = CredentialRelay::new(tmp.path().join("credentials"));
        let sessions_base = tmp.path().join("sessions");
        let hydrator = WorkspaceHydrator::new(source, assembler, relay, sessions_base.clone());

        let session_id = SessionId::new();
        let config = make_assembly_config(
            &tmp,
            session_id.clone(),
            "file:///nonexistent-repo-that-does-not-exist.git",
        );

        let result = hydrator.hydrate(
            &session_id,
            "alice",
            &[("file:///nonexistent-repo-that-does-not-exist.git", "proj")],
            config,
        );

        assert!(result.is_err(), "hydration with bad repo URL must fail");
        // No partial workspace directory may remain
        let expected_ws = sessions_base.join("alice").join(session_id.as_str());
        assert!(
            !expected_ws.exists(),
            "partial workspace must be removed after failed hydration"
        );
    }

    // ── AC-4 error message content ──────────────────────────────────────────

    #[test]
    fn hydration_error_includes_operation_and_cause() {
        let tmp = TempDir::new().unwrap();
        let source = git_worktree_source(&tmp);
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let relay = CredentialRelay::new(tmp.path().join("credentials"));
        let hydrator =
            WorkspaceHydrator::new(source, assembler, relay, tmp.path().join("sessions"));

        let session_id = SessionId::new();
        let config = make_assembly_config(&tmp, session_id.clone(), "file:///no-such-repo.git");

        let err = hydrator
            .hydrate(
                &session_id,
                "alice",
                &[("file:///no-such-repo.git", "proj")],
                config,
            )
            .unwrap_err();

        let msg = err.to_string();
        // AC-4: error must describe the failing operation and underlying cause
        assert!(!msg.is_empty(), "error message must be non-empty");
    }

    // ── AC-5: Skills Available ──────────────────────────────────────────────

    #[test]
    fn config_assembler_reports_missing_skill_dirs_as_warnings() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let session_id = SessionId::new();

        let nonexistent = tmp.path().join("skills").join("missing-dir");
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![nonexistent.clone()],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        let warnings = assembler.assemble(&workspace, &config).unwrap();
        // AC-5: missing skill dir must appear in warnings, not cause an error
        assert!(
            warnings.iter().any(|w| w.contains("missing-dir")
                || w.contains("not found")
                || w.contains("missing")),
            "missing skill dir must be reported as a warning, got: {:?}",
            warnings
        );
    }

    // ── AC-6: Credentials Accessible ────────────────────────────────────────

    #[test]
    fn credential_relay_returns_accessible_directory_for_configured_member() {
        let tmp = TempDir::new().unwrap();
        let creds_base = tmp.path().join("credentials");
        let member_dir = creds_base.join("alice");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(member_dir.join("hosts.yml"), "# mock token\n").unwrap();

        let relay = CredentialRelay::new(creds_base);
        let path = relay.credential_path("alice").unwrap();

        assert!(path.exists(), "credential path must exist");
        assert!(path.is_dir(), "credential path must be a directory");
    }

    #[test]
    fn credential_relay_errors_for_member_with_no_credentials() {
        let tmp = TempDir::new().unwrap();
        let creds_base = tmp.path().join("credentials");
        fs::create_dir_all(&creds_base).unwrap();

        let relay = CredentialRelay::new(creds_base);
        let result = relay.credential_path("nonexistent-member");

        assert!(
            result.is_err(),
            "credential relay must error for a member with no credentials configured"
        );
    }

    // ── AC-7: Shared Clone Freshness ─────────────────────────────────────────

    #[test]
    fn provision_fetches_before_worktree_when_clone_is_stale() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        // Zero threshold means any clone is considered stale — fetch always runs
        let source = GitWorktreeSource::new(tmp.path().join("clones"), Duration::from_secs(0));
        let target = tmp.path().join("ws");

        // Must succeed: fetch ran and worktree was created from fresh state
        source
            .provision(repo.to_str().unwrap(), None, &target)
            .unwrap();

        assert!(
            target.exists(),
            "workspace must exist after provision with zero freshness threshold"
        );
    }

    // ── Config Assembler idempotency ─────────────────────────────────────────

    #[test]
    fn config_assembler_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let session_id = SessionId::new();

        let config = AssemblyConfig {
            session_id: session_id.clone(),
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: Some(42),
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        let warnings_first = assembler.assemble(&workspace, &config).unwrap();
        let warnings_second = assembler.assemble(&workspace, &config).unwrap();

        assert_eq!(
            warnings_first, warnings_second,
            "config assembler must be idempotent — same result on second call"
        );
    }

    // ── Workspace marker contains session ID ─────────────────────────────────

    #[test]
    fn assembled_workspace_marker_contains_session_id() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let session_id = SessionId::new();

        let config = AssemblyConfig {
            session_id: session_id.clone(),
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: Some(42),
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        let marker = fs::read_to_string(workspace.join(".botminter.workspace")).unwrap();
        assert!(
            marker.contains(session_id.as_str()),
            "workspace marker must contain session ID '{}', got:\n{}",
            session_id,
            marker
        );
    }

    // ── brain-prompt.md assembly ─────────────────────────────────────────────

    #[test]
    fn assemble_copies_brain_prompt_when_present_in_team_member_dir() {
        let tmp = TempDir::new().unwrap();
        let member_dir = tmp.path().join("team/members/alice");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(member_dir.join("brain-prompt.md"), "# Brain Prompt").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join("brain-prompt.md").exists(),
            "brain-prompt.md must be copied to session workspace when present in team member dir"
        );
        let content = fs::read_to_string(workspace.join("brain-prompt.md")).unwrap();
        assert_eq!(content, "# Brain Prompt");
    }

    #[test]
    fn assemble_skips_brain_prompt_when_absent_from_team_member_dir() {
        let tmp = TempDir::new().unwrap();
        let member_dir = tmp.path().join("team/members/alice");
        fs::create_dir_all(&member_dir).unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            !workspace.join("brain-prompt.md").exists(),
            "brain-prompt.md must not appear in session workspace when absent from team member dir"
        );
    }

    // ── dm-room.json assembly ────────────────────────────────────────────────

    #[test]
    fn dm_room_json_copied_from_member_state_into_session_workspace() {
        let tmp = TempDir::new().unwrap();
        let member_state_dir = tmp.path().join("member-state/alice");
        fs::create_dir_all(&member_state_dir).unwrap();
        fs::write(
            member_state_dir.join("dm-room.json"),
            r#"{"room_id":"!abc123:example.com"}"#,
        )
        .unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join("dm-room.json").exists(),
            "dm-room.json must be copied to session workspace when present at member state path"
        );
        let content = fs::read_to_string(workspace.join("dm-room.json")).unwrap();
        assert_eq!(content, r#"{"room_id":"!abc123:example.com"}"#);
    }

    // ── AC-08: .claude/ assembly ─────────────────────────────────────────────

    #[test]
    fn assemble_creates_claude_agents_symlink_from_team_coding_agent() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        let agents_src = team.join("coding-agent/agents");
        fs::create_dir_all(&agents_src).unwrap();
        fs::write(agents_src.join("finalization.md"), "# Finalization").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        let agent_file = workspace.join(".claude/agents/finalization.md");
        assert!(
            agent_file.exists(),
            ".claude/agents/finalization.md must exist after assemble()"
        );
        assert!(
            agent_file
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false),
            ".claude/agents/finalization.md must be a symlink"
        );
    }

    #[test]
    fn assemble_creates_claude_skills_symlinks_from_team_coding_agent() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        let skill_src = team.join("coding-agent/skills/my-skill");
        fs::create_dir_all(&skill_src).unwrap();
        fs::write(skill_src.join("SKILL.md"), "# My Skill").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        let skills_dir = workspace.join(".claude/skills");
        assert!(
            skills_dir.exists(),
            ".claude/skills/ must exist after assemble()"
        );
        assert!(
            skills_dir.join("my-skill").symlink_metadata().is_ok(),
            ".claude/skills/my-skill must exist as a symlink after assemble()"
        );
    }

    #[test]
    fn assemble_copies_team_settings_json_into_claude_dir() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        let coding_agent_dir = team.join("coding-agent");
        fs::create_dir_all(&coding_agent_dir).unwrap();
        fs::write(coding_agent_dir.join("settings.json"), r#"{"hooks": {}}"#).unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join(".claude/settings.json").exists(),
            ".claude/settings.json must exist after assemble()"
        );
    }

    #[test]
    fn assemble_completes_and_creates_claude_dir_when_no_coding_agent_dir() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        fs::create_dir_all(&team).unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        let result = assembler.assemble(&workspace, &config);
        assert!(
            result.is_ok(),
            "assemble() must not error when no coding-agent dir exists"
        );
        assert!(
            workspace.join(".claude").is_dir(),
            ".claude/ must be created by assemble() even when team has no coding-agent dir"
        );
    }

    // ── AC-08: Member-level .claude/ assembly ───────────────────────────────

    #[test]
    fn assemble_includes_member_level_skill_in_claude_skills_dir() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        let member_skill_src = team.join("members/alice/coding-agent/skills/story-mgmt");
        fs::create_dir_all(&member_skill_src).unwrap();
        fs::write(member_skill_src.join("SKILL.md"), "# Story Mgmt").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join(".claude/skills/story-mgmt").exists(),
            ".claude/skills/story-mgmt must be present — member-level skill must be assembled \
             from team/members/alice/coding-agent/skills/"
        );
    }

    #[test]
    fn assemble_merges_team_and_member_skills_in_claude_skills_dir() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        let team_skill_src = team.join("coding-agent/skills/ro-loop");
        fs::create_dir_all(&team_skill_src).unwrap();
        fs::write(team_skill_src.join("SKILL.md"), "# ro-loop").unwrap();
        let member_skill_src = team.join("members/alice/coding-agent/skills/story-mgmt");
        fs::create_dir_all(&member_skill_src).unwrap();
        fs::write(member_skill_src.join("SKILL.md"), "# story-mgmt").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join(".claude/skills/ro-loop").exists(),
            ".claude/skills/ro-loop must be present — team-level skill"
        );
        assert!(
            workspace.join(".claude/skills/story-mgmt").exists(),
            ".claude/skills/story-mgmt must be present — member-level skill must be assembled \
             alongside team-level skills"
        );
    }

    // ── AC-09: Credential write-path ─────────────────────────────────────────

    struct TestCredentialWriter {
        token: String,
    }

    impl CredentialWriter for TestCredentialWriter {
        fn write_credentials(&self, member_dir: &Path) -> Result<()> {
            fs::create_dir_all(member_dir)?;
            fs::write(member_dir.join("hosts.yml"), super::hosts_yml_content(&self.token))?;
            Ok(())
        }
    }

    #[test]
    fn ensure_credentials_writes_hosts_yml_to_member_credential_dir() {
        let tmp = TempDir::new().unwrap();
        let creds_base = tmp.path().join("credentials");
        let relay = CredentialRelay::with_writer(
            creds_base.clone(),
            Box::new(TestCredentialWriter {
                token: "test-token".to_string(),
            }),
        );

        relay.ensure_credentials("alice").unwrap();

        let hosts_yml = creds_base.join("alice").join("hosts.yml");
        assert!(
            hosts_yml.exists(),
            "hosts.yml must exist at the shared credential path after ensure_credentials()"
        );
    }

    #[test]
    fn credential_path_contains_hosts_yml_after_ensure_credentials() {
        let tmp = TempDir::new().unwrap();
        let creds_base = tmp.path().join("credentials");
        // Pre-create the member dir so credential_path() does not fail on missing dir.
        fs::create_dir_all(creds_base.join("alice")).unwrap();

        let relay = CredentialRelay::with_writer(
            creds_base.clone(),
            Box::new(TestCredentialWriter {
                token: "test-token".to_string(),
            }),
        );

        relay.ensure_credentials("alice").unwrap();
        let path = relay.credential_path("alice").unwrap();

        assert!(
            path.join("hosts.yml").exists(),
            "credential_path() must point to a directory containing hosts.yml after ensure_credentials()"
        );
    }

    #[test]
    fn hydrate_writes_hosts_yml_to_shared_credential_path() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        let creds_base = tmp.path().join("credentials");
        let source = git_worktree_source(&tmp);
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let relay = CredentialRelay::with_writer(
            creds_base.clone(),
            Box::new(TestCredentialWriter {
                token: "test-token".to_string(),
            }),
        );
        let hydrator =
            WorkspaceHydrator::new(source, assembler, relay, tmp.path().join("sessions"));

        let session_id = SessionId::new();
        let config = make_assembly_config(&tmp, session_id.clone(), repo.to_str().unwrap());

        hydrator
            .hydrate(
                &session_id,
                "alice",
                &[(repo.to_str().unwrap(), "myproject")],
                config,
            )
            .unwrap();

        let hosts_yml = creds_base.join("alice").join("hosts.yml");
        assert!(
            hosts_yml.exists(),
            "hosts.yml must exist at the shared credential path after hydrate()"
        );
    }

    #[test]
    fn hydrate_proceeds_without_error_when_no_credential_writer() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        let source = git_worktree_source(&tmp);
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let relay = CredentialRelay::new(tmp.path().join("credentials")); // NoOp — no app creds
        let hydrator =
            WorkspaceHydrator::new(source, assembler, relay, tmp.path().join("sessions"));

        let session_id = SessionId::new();
        let config = make_assembly_config(&tmp, session_id.clone(), repo.to_str().unwrap());

        let result = hydrator.hydrate(
            &session_id,
            "alice",
            &[(repo.to_str().unwrap(), "myproject")],
            config,
        );

        assert!(
            result.is_ok(),
            "hydrate() must succeed even when no credential writer is configured (non-fatal)"
        );
    }

    // ── AC-09: Production wiring — HydrationWorkspaceOps ────────────────────

    #[derive(Debug)]
    struct MockTokenProvider {
        token: String,
    }

    impl AppTokenProvider for MockTokenProvider {
        fn resolve_token(&self, _member_name: &str) -> Result<Option<String>> {
            Ok(Some(self.token.clone()))
        }
    }

    #[test]
    fn workspace_ops_writes_hosts_yml_when_token_provider_resolves_token() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "project");
        let creds_base = tmp.path().join("credentials");

        let config = HydrationWorkspaceConfig {
            clones_dir: tmp.path().join("clones"),
            sessions_base: tmp.path().join("sessions"),
            team_repo_path: tmp.path().join("team"),
            credential_base: creds_base.clone(),
            freshness_threshold: Duration::from_secs(300),
            repo_urls: vec![(
                repo.to_str().unwrap().to_string(),
                "project".to_string(),
            )],
            team_repo_url: repo.to_str().unwrap().to_string(),
            team_repo_branch: "main".to_string(),
            workspace_base: tmp.path().join("workspace"),
            project_number: None,
            skill_dirs: vec![],
            credential_resolver: Some(std::sync::Arc::new(MockTokenProvider {
                token: "ghs_test_token_abc123".to_string(),
            })),
            project_names: vec![],
        };

        let ops = HydrationWorkspaceOps::new(config);
        let session_id = SessionId::new();
        ops.hydrate_workspace(&session_id, "alice").unwrap();

        // When a token provider resolves a token, the production HydrationWorkspaceOps
        // MUST write hosts.yml to <credential_base>/<member>/gh/hosts.yml (D-02 shared path).
        let hosts_yml = creds_base.join("alice").join("gh").join("hosts.yml");
        assert!(
            hosts_yml.exists(),
            "hosts.yml must exist at <credential_base>/alice/gh/hosts.yml after \
             hydrate_workspace() when the token provider resolves a token — \
             NoOpCredentialWriter was used instead of AppCredentialWriter"
        );
    }

    // ── Deprovision removes directory ────────────────────────────────────────

    #[test]
    fn deprovision_removes_working_tree_directory() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        let source = git_worktree_source(&tmp);
        let target = tmp.path().join("ws");

        source
            .provision(repo.to_str().unwrap(), None, &target)
            .unwrap();
        assert!(
            target.exists(),
            "precondition: workspace must exist after provision"
        );

        source.deprovision(&target).unwrap();

        assert!(
            !target.exists(),
            "workspace directory must be removed after deprovision"
        );
    }

    // ── AC-09: Shared credential path (D-02) ────────────────────────────────

    #[test]
    fn gh_config_dir_for_member_returns_shared_credential_path() {
        let tmp = TempDir::new().unwrap();
        let creds_base = tmp.path().join("credentials");

        // Place hosts.yml at the D-02 shared path: <creds_base>/alice/gh/hosts.yml
        let shared_gh_dir = creds_base.join("alice").join("gh");
        fs::create_dir_all(&shared_gh_dir).unwrap();
        fs::write(
            shared_gh_dir.join("hosts.yml"),
            "github.com:\n  oauth_token: test\n",
        )
        .unwrap();

        let config = HydrationWorkspaceConfig {
            clones_dir: tmp.path().join("clones"),
            sessions_base: tmp.path().join("sessions"),
            team_repo_path: tmp.path().join("team"),
            credential_base: creds_base.clone(),
            freshness_threshold: Duration::from_secs(300),
            repo_urls: vec![],
            team_repo_url: String::new(),
            team_repo_branch: "main".to_string(),
            workspace_base: tmp.path().join("workspace"),
            project_number: None,
            skill_dirs: vec![],
            credential_resolver: None,
            project_names: vec![],
        };
        let ops = HydrationWorkspaceOps::new(config);

        let result = ops.gh_config_dir_for_member("alice");

        assert_eq!(
            result,
            Some(shared_gh_dir),
            "gh_config_dir_for_member must return the D-02 shared path \
             <credential_base>/alice/gh, not workspace_base/alice/.config/gh"
        );
    }

    #[test]
    fn hydrate_session_writes_hosts_yml_to_gh_subdir_of_shared_credential_path() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "project");
        let creds_base = tmp.path().join("credentials");

        let config = HydrationWorkspaceConfig {
            clones_dir: tmp.path().join("clones"),
            sessions_base: tmp.path().join("sessions"),
            team_repo_path: tmp.path().join("team"),
            credential_base: creds_base.clone(),
            freshness_threshold: Duration::from_secs(300),
            repo_urls: vec![(
                repo.to_str().unwrap().to_string(),
                "project".to_string(),
            )],
            team_repo_url: repo.to_str().unwrap().to_string(),
            team_repo_branch: "main".to_string(),
            workspace_base: tmp.path().join("workspace"),
            project_number: None,
            skill_dirs: vec![],
            credential_resolver: Some(std::sync::Arc::new(MockTokenProvider {
                token: "ghs_test_token".to_string(),
            })),
            project_names: vec![],
        };
        let ops = HydrationWorkspaceOps::new(config);
        let session_id = SessionId::new();
        ops.hydrate_workspace(&session_id, "alice").unwrap();

        // D-02 path requires a gh/ subdirectory: <credential_base>/alice/gh/hosts.yml
        let expected = creds_base.join("alice").join("gh").join("hosts.yml");
        assert!(
            expected.exists(),
            "hosts.yml MUST be written to <credential_base>/alice/gh/hosts.yml \
             (D-02 shared path with gh/ subdir), not <credential_base>/alice/hosts.yml; \
             AppCredentialWriter must create the gh/ subdirectory"
        );
    }

    // ── AC-08: Project-level .claude/ assembly ───────────────────────────────

    #[test]
    fn assemble_merges_project_level_agents_into_claude_agents_dir() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        // Project-level agent at team/projects/botminter/coding-agent/agents/pr-review.md
        let project_agent_src = team.join("projects/botminter/coding-agent/agents");
        fs::create_dir_all(&project_agent_src).unwrap();
        fs::write(project_agent_src.join("pr-review.md"), "# PR Review").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec!["botminter".to_string()],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join(".claude/agents/pr-review.md").exists(),
            ".claude/agents/pr-review.md must exist — project-level agent from \
             team/projects/botminter/coding-agent/agents/ must be merged into .claude/agents/"
        );
    }

    #[test]
    fn assemble_merges_project_level_skills_into_claude_skills_dir() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        // Project-level skill at team/projects/botminter/coding-agent/skills/code-review/
        let project_skill_src = team.join("projects/botminter/coding-agent/skills/code-review");
        fs::create_dir_all(&project_skill_src).unwrap();
        fs::write(project_skill_src.join("SKILL.md"), "# Code Review").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec!["botminter".to_string()],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join(".claude/skills/code-review").exists(),
            ".claude/skills/code-review must exist — project-level skill from \
             team/projects/botminter/coding-agent/skills/ must be merged into .claude/skills/"
        );
    }

    #[test]
    fn assemble_creates_claude_commands_from_member_coding_agent() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        // Member-level command at team/members/alice/coding-agent/commands/my-cmd.md
        let member_cmds_src = team.join("members/alice/coding-agent/commands");
        fs::create_dir_all(&member_cmds_src).unwrap();
        fs::write(member_cmds_src.join("my-cmd.md"), "# My Command").unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join(".claude/commands/my-cmd.md").exists(),
            ".claude/commands/my-cmd.md must exist — member-level command from \
             team/members/alice/coding-agent/commands/ must be assembled into .claude/commands/"
        );
    }

    #[test]
    fn assemble_copies_member_settings_local_json_into_claude_dir() {
        let tmp = TempDir::new().unwrap();
        let team = tmp.path().join("team");
        // Member-level settings.local.json at team/members/alice/coding-agent/settings.local.json
        let member_coding_agent = team.join("members/alice/coding-agent");
        fs::create_dir_all(&member_coding_agent).unwrap();
        fs::write(
            member_coding_agent.join("settings.local.json"),
            r#"{"permissions": {"allow": ["Bash"]}}"#,
        )
        .unwrap();

        let workspace = tmp.path().join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let assembler = ConfigAssembler::new(team, "alice".to_string());
        let session_id = SessionId::new();
        let config = AssemblyConfig {
            session_id,
            member_name: "alice".to_string(),
            team_repo_url: "https://example.com/team.git".to_string(),
            team_repo_branch: "main".to_string(),
            project_number: None,
            skill_dirs: vec![],
            credential_base: tmp.path().join("credentials"),
            project_names: vec![],
            workspace_base: tmp.path().to_path_buf(),
        };

        assembler.assemble(&workspace, &config).unwrap();

        assert!(
            workspace.join(".claude/settings.local.json").exists(),
            ".claude/settings.local.json must exist — member-level settings.local.json from \
             team/members/alice/coding-agent/settings.local.json must be copied into .claude/"
        );
    }

    // ── AC-08: Production wiring — project_names flows through HydrationWorkspaceOps ──

    #[test]
    fn hydrate_workspace_includes_project_level_agents_when_project_names_configured() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "project");

        // Set up team repo with a project-level coding-agent agent.
        let team = tmp.path().join("team");
        let project_agent_src = team.join("projects/botminter/coding-agent/agents");
        fs::create_dir_all(&project_agent_src).unwrap();
        fs::write(project_agent_src.join("pr-review.md"), "# PR Review").unwrap();

        let config = HydrationWorkspaceConfig {
            clones_dir: tmp.path().join("clones"),
            sessions_base: tmp.path().join("sessions"),
            team_repo_path: team,
            credential_base: tmp.path().join("credentials"),
            freshness_threshold: Duration::from_secs(300),
            repo_urls: vec![(repo.to_str().unwrap().to_string(), "project".to_string())],
            team_repo_url: repo.to_str().unwrap().to_string(),
            team_repo_branch: "main".to_string(),
            workspace_base: tmp.path().join("workspace"),
            project_number: None,
            skill_dirs: vec![],
            credential_resolver: None,
            project_names: vec!["botminter".to_string()],
        };

        let ops = HydrationWorkspaceOps::new(config);
        let session_id = SessionId::new();
        ops.hydrate_workspace(&session_id, "alice").unwrap();

        // The assembled workspace must include the project-level agent.
        // FAILS: hydrate_workspace() hardcodes project_names: vec![] in AssemblyConfig,
        // ignoring the project_names stored in HydrationWorkspaceOps.
        let ws = tmp.path().join("sessions").join("alice").join(session_id.as_str());
        assert!(
            ws.join(".claude/agents/pr-review.md").exists(),
            ".claude/agents/pr-review.md must exist — project_names in \
             HydrationWorkspaceConfig must be passed through hydrate_workspace() \
             to AssemblyConfig; currently hydrate_workspace() hardcodes \
             project_names: vec![] so project-level agents are never assembled"
        );
    }

    // ── Layout invariant: .botminter.workspace marker present ───────────────

    #[test]
    fn hydrated_workspace_contains_workspace_marker() {
        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "myproject");
        let source = git_worktree_source(&tmp);
        let assembler = ConfigAssembler::new(tmp.path().join("team"), "alice".to_string());
        let relay = CredentialRelay::new(tmp.path().join("credentials"));
        let hydrator =
            WorkspaceHydrator::new(source, assembler, relay, tmp.path().join("sessions"));

        let session_id = SessionId::new();
        let config = make_assembly_config(&tmp, session_id.clone(), repo.to_str().unwrap());

        let result = hydrator
            .hydrate(
                &session_id,
                "alice",
                &[(repo.to_str().unwrap(), "myproject")],
                config,
            )
            .unwrap();

        assert!(
            result.workspace_path.join(".botminter.workspace").exists(),
            ".botminter.workspace marker must exist in the hydrated workspace"
        );
    }

    // ── CT-154-03: AppCredentialWriter must use atomic write ─────────────────

    #[test]
    fn app_credential_writer_overwrites_readonly_hosts_yml_atomically() {
        let tmp = TempDir::new().unwrap();
        let member_dir = tmp.path().join("alice");
        let gh_dir = member_dir.join("gh");
        fs::create_dir_all(&gh_dir).unwrap();

        // Create an existing read-only hosts.yml in a writable directory.
        // Atomic rename can replace a read-only file when the parent dir is writable.
        // Non-atomic fs::write opens the file directly → EACCES on read-only.
        let hosts_yml = gh_dir.join("hosts.yml");
        fs::write(&hosts_yml, "github.com:\n    oauth_token: old_token\n").unwrap();
        let mut perms = fs::metadata(&hosts_yml).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&hosts_yml, perms).unwrap();

        let writer = AppCredentialWriter {
            provider: std::sync::Arc::new(MockTokenProvider {
                token: "new_token".to_string(),
            }),
        };

        let result = writer.write_credentials(&member_dir);

        // Restore write permission so TempDir cleanup can remove the file.
        if let Ok(meta) = fs::metadata(&hosts_yml) {
            let mut p = meta.permissions();
            p.set_readonly(false);
            let _ = fs::set_permissions(&hosts_yml, p);
        }

        assert!(
            result.is_ok(),
            "write_credentials must succeed even when existing hosts.yml is read-only; \
             use atomic write (write to temp file + fs::rename) instead of fs::write"
        );
    }

    #[test]
    fn keyring_token_provider_resolves_token_when_credentials_are_wired() {
        use crate::formation::InMemoryKeyValueCredentialStore;
        use crate::git::manifest_flow::credential_keys;

        let store = std::sync::Arc::new(InMemoryKeyValueCredentialStore::new());
        store.store(&credential_keys::client_id("alice"), "fake-client-id").unwrap();
        store.store(&credential_keys::private_key("alice"), "fake-private-key").unwrap();
        store.store(&credential_keys::installation_id("alice"), "12345").unwrap();

        let provider = KeyringAppTokenProvider::new(store);
        let token = provider.resolve_token("alice").unwrap();

        assert!(
            token.is_some(),
            "KeyringAppTokenProvider::resolve_token must return Some(token) when the credential \
             store has client_id, private_key, and installation_id for the member — got None"
        );
    }

    #[test]
    fn hydration_with_keyring_provider_writes_hosts_yml_at_d02_path() {
        use crate::formation::InMemoryKeyValueCredentialStore;
        use crate::git::manifest_flow::credential_keys;

        let tmp = TempDir::new().unwrap();
        let repo = init_bare_repo(&tmp, "project");
        let creds_base = tmp.path().join("credentials");

        let store = std::sync::Arc::new(InMemoryKeyValueCredentialStore::new());
        store.store(&credential_keys::client_id("alice"), "fake-client-id").unwrap();
        store.store(&credential_keys::private_key("alice"), "fake-private-key").unwrap();
        store.store(&credential_keys::installation_id("alice"), "12345").unwrap();

        let config = HydrationWorkspaceConfig {
            clones_dir: tmp.path().join("clones"),
            sessions_base: tmp.path().join("sessions"),
            team_repo_path: tmp.path().join("team"),
            credential_base: creds_base.clone(),
            freshness_threshold: Duration::from_secs(300),
            repo_urls: vec![(
                repo.to_str().unwrap().to_string(),
                "project".to_string(),
            )],
            team_repo_url: repo.to_str().unwrap().to_string(),
            team_repo_branch: "main".to_string(),
            workspace_base: tmp.path().join("workspace"),
            project_number: None,
            skill_dirs: vec![],
            credential_resolver: Some(std::sync::Arc::new(KeyringAppTokenProvider::new(store))),
            project_names: vec![],
        };

        let ops = HydrationWorkspaceOps::new(config);
        let session_id = SessionId::new();
        ops.hydrate_workspace(&session_id, "alice").unwrap();

        let hosts_yml = creds_base.join("alice").join("gh").join("hosts.yml");
        assert!(
            hosts_yml.exists(),
            "hosts.yml must be written to <credential_base>/alice/gh/hosts.yml when \
             KeyringAppTokenProvider is used as credential_resolver and store has credentials"
        );
    }
}
