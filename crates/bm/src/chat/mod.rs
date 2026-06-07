pub(crate) mod config;
pub(crate) mod skills;
pub mod spawn;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};

pub use config::{read_member_info, RalphConfig};
pub use skills::scan_skills;

/// A skill available for loading on demand during a chat session.
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub load_command: String,
}

/// Parameters for building a meta-prompt for `bm chat`.
pub struct MetaPromptParams<'a> {
    pub member_name: &'a str,
    pub role_name: &'a str,
    pub role_description: &'a str,
    pub team_name: &'a str,
    pub guardrails: &'a [String],
    pub hat_instructions: &'a BTreeMap<String, String>,
    pub prompt_md_content: &'a str,
    pub reference_dir: &'a str,
    pub hat: Option<&'a str>,
    pub skills: &'a [SkillInfo],
}

/// All data needed to launch a coding agent session.
pub struct AgentSession {
    /// The assembled meta-prompt markdown.
    pub meta_prompt: String,
    /// Path to the member's workspace.
    pub ws_path: std::path::PathBuf,
    /// D-02 shared credential directory for ephemeral sessions.
    /// `sessions_base/credentials/<member>` (no `/gh` suffix — appended internally).
    /// `None` for legacy permanent workspace sessions (deprecated).
    pub credential_dir: Option<std::path::PathBuf>,
}

/// Prepares a chat session from a pre-existing session workspace path.
///
/// Unlike [`prepare_chat_session`], this variant accepts the workspace path
/// directly (e.g., an ephemeral session workspace returned by the daemon) and
/// does NOT check for a `.botminter.workspace` marker.
pub fn prepare_chat_session_from_path(
    team_repo: &Path,
    team_name: &str,
    member: &str,
    workspace_path: &Path,
    hat: Option<&str>,
) -> Result<AgentSession> {
    // Verify member exists in team repo
    let member_dir = team_repo.join("members").join(member);
    if !member_dir.is_dir() {
        bail!(
            "Member '{}' not found in team '{}'. \
             Run `bm members list` to see hired members.",
            member, team_name
        );
    }

    // Read ralph.yml from session workspace
    let ralph_yml_path = workspace_path.join("ralph.yml");
    let ralph_contents = std::fs::read_to_string(&ralph_yml_path)
        .with_context(|| format!("Failed to read {}", ralph_yml_path.display()))?;
    let ralph_config: RalphConfig = serde_yml::from_str(&ralph_contents)
        .with_context(|| format!("Failed to parse {}", ralph_yml_path.display()))?;

    // Read PROMPT.md from session workspace
    let prompt_md_path = workspace_path.join("PROMPT.md");
    let prompt_md_content = std::fs::read_to_string(&prompt_md_path)
        .with_context(|| format!("Failed to read {}", prompt_md_path.display()))?;

    // Read member info from team repo
    let (role_name, display_name) = read_member_info(&member_dir, member)?;

    // Extract hat instructions and validate --hat flag
    let hat_instructions: BTreeMap<String, String> = ralph_config
        .hats
        .into_iter()
        .filter_map(|(name, h)| h.instructions.map(|instr| (name, instr)))
        .collect();

    if let Some(hat_name) = hat {
        if !hat_instructions.contains_key(hat_name) {
            if hat_instructions.is_empty() {
                bail!(
                    "Hat '{}' not found for member '{}'. \
                     No hats with instructions found in ralph.yml",
                    hat_name, member
                );
            } else {
                let mut available: Vec<&str> =
                    hat_instructions.keys().map(|k| k.as_str()).collect();
                available.sort();
                bail!(
                    "Hat '{}' not found for member '{}'. Available hats: {}",
                    hat_name, member, available.join(", ")
                );
            }
        }
    }

    // Load manifest for role description
    let manifest = crate::profile::read_team_repo_manifest(team_repo)?;
    let role_description = manifest
        .roles
        .iter()
        .find(|r| r.name == role_name)
        .map(|r| r.description.as_str())
        .unwrap_or("");

    // Scan skills in session workspace
    let skills = if ralph_config.skills.enabled {
        scan_skills(workspace_path, &ralph_config.skills.dirs)
    } else {
        Vec::new()
    };

    // Build meta-prompt
    let params = MetaPromptParams {
        member_name: &display_name,
        role_name: &role_name,
        role_description,
        team_name,
        guardrails: &ralph_config.core.guardrails,
        hat_instructions: &hat_instructions,
        prompt_md_content: &prompt_md_content,
        reference_dir: "team/ralph-prompts/reference/",
        hat,
        skills: &skills,
    };
    let meta_prompt = build_meta_prompt(&params);

    let daemon_paths = crate::daemon::DaemonPaths::new(team_name)?;
    let credential_dir = daemon_paths
        .sessions_base()
        .join("credentials")
        .join(member);

    Ok(AgentSession {
        meta_prompt,
        ws_path: workspace_path.to_path_buf(),
        credential_dir: Some(credential_dir),
    })
}

/// Prepares all data for a `bm chat` session: validates the member and
/// workspace exist, reads ralph.yml and PROMPT.md, validates hat flags,
/// scans skills, and builds the meta-prompt.
pub fn prepare_chat_session(
    team_repo: &Path,
    team_name: &str,
    team_path: &Path,
    member: &str,
    hat: Option<&str>,
) -> Result<AgentSession> {
    // Verify member exists
    let member_dir = team_repo.join("members").join(member);
    if !member_dir.is_dir() {
        bail!(
            "Member '{}' not found in team '{}'. \
             Run `bm members list` to see hired members.",
            member, team_name
        );
    }

    // Find workspace
    let ws_path = team_path.join(member);
    if !ws_path.join(".botminter.workspace").exists() {
        bail!(
            "No workspace found for member '{}'. \
             Run `bm teams sync` first.",
            member
        );
    }

    // Read ralph.yml
    let ralph_yml_path = ws_path.join("ralph.yml");
    let ralph_contents = std::fs::read_to_string(&ralph_yml_path)
        .with_context(|| format!("Failed to read {}", ralph_yml_path.display()))?;
    let ralph_config: RalphConfig = serde_yml::from_str(&ralph_contents)
        .with_context(|| format!("Failed to parse {}", ralph_yml_path.display()))?;

    // Read PROMPT.md
    let prompt_md_path = ws_path.join("PROMPT.md");
    let prompt_md_content = std::fs::read_to_string(&prompt_md_path)
        .with_context(|| format!("Failed to read {}", prompt_md_path.display()))?;

    // Read member info
    let (role_name, display_name) = read_member_info(&member_dir, member)?;

    // Extract hat instructions and validate --hat flag
    let hat_instructions: BTreeMap<String, String> = ralph_config
        .hats
        .into_iter()
        .filter_map(|(name, h)| h.instructions.map(|instr| (name, instr)))
        .collect();

    if let Some(hat_name) = hat {
        if !hat_instructions.contains_key(hat_name) {
            if hat_instructions.is_empty() {
                bail!(
                    "Hat '{}' not found for member '{}'. \
                     No hats with instructions found in ralph.yml",
                    hat_name, member
                );
            } else {
                let mut available: Vec<&str> =
                    hat_instructions.keys().map(|k| k.as_str()).collect();
                available.sort();
                bail!(
                    "Hat '{}' not found for member '{}'. Available hats: {}",
                    hat_name, member, available.join(", ")
                );
            }
        }
    }

    // Load manifest for role description
    let manifest = crate::profile::read_team_repo_manifest(team_repo)?;
    let role_description = manifest
        .roles
        .iter()
        .find(|r| r.name == role_name)
        .map(|r| r.description.as_str())
        .unwrap_or("");

    // Scan skills
    let skills = if ralph_config.skills.enabled {
        scan_skills(&ws_path, &ralph_config.skills.dirs)
    } else {
        Vec::new()
    };

    // Build meta-prompt
    let params = MetaPromptParams {
        member_name: &display_name,
        role_name: &role_name,
        role_description,
        team_name,
        guardrails: &ralph_config.core.guardrails,
        hat_instructions: &hat_instructions,
        prompt_md_content: &prompt_md_content,
        reference_dir: "team/ralph-prompts/reference/",
        hat,
        skills: &skills,
    };
    let meta_prompt = build_meta_prompt(&params);

    Ok(AgentSession { meta_prompt, ws_path, credential_dir: None })
}

/// Builds a meta-prompt for an interactive `bm chat` session.
///
/// Assembles role identity, hat capabilities, guardrails, role context,
/// and reference paths into a single markdown document. Supports two modes:
/// - Hatless (hat=None): all hats' instructions included
/// - Hat-specific (hat=Some("executor")): only that hat's instructions
pub fn build_meta_prompt(params: &MetaPromptParams) -> String {
    let mut out = String::new();

    // Header: role identity
    out.push_str(&format!(
        "# Interactive Session — {}\n",
        params.member_name
    ));
    out.push('\n');
    out.push_str(&format!(
        "You are a member of the {} team.\n",
        params.team_name
    ));
    out.push_str(&format!("Your name is {}.\n", params.member_name));
    out.push_str(&format!("Your role is called {}.\n", params.role_name));
    if !params.role_description.is_empty() {
        out.push_str(&format!(
            "Your role description is: {}\n",
            params.role_description
        ));
    }
    out.push_str("You normally run autonomously inside Ralph Orchestrator.\n");
    out.push_str("Right now you are in an interactive session with the human (PO).\n");

    // Your Capabilities section
    out.push('\n');
    out.push_str("## Your Capabilities\n");
    out.push('\n');

    match params.hat {
        Some(hat_name) => {
            if let Some(instructions) = params.hat_instructions.get(hat_name) {
                out.push_str(instructions.trim_end());
                out.push('\n');
            }
        }
        None => {
            // Hatless mode: include all hats (BTreeMap gives sorted order)
            for (name, instructions) in params.hat_instructions {
                out.push_str(&format!("### {}\n", name));
                out.push('\n');
                out.push_str(instructions.trim_end());
                out.push_str("\n\n");
            }
        }
    }

    // Skills section (only if skills are available)
    if !params.skills.is_empty() {
        out.push_str("## Skills\n");
        out.push('\n');
        out.push_str("Available skills you can load on demand:\n");
        out.push('\n');
        out.push_str("| Skill | Description | Load Command |\n");
        out.push_str("|-------|-------------|---------------|\n");
        for skill in params.skills {
            out.push_str(&format!(
                "| {} | {} | Read `{}` |\n",
                skill.name, skill.description, skill.load_command
            ));
        }
        out.push('\n');
        out.push_str("To use a skill, read its SKILL.md file for full instructions.\n");
        out.push('\n');
    }

    // Guardrails section
    out.push_str("## Guardrails\n");
    out.push('\n');
    for (i, guardrail) in params.guardrails.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", 999 + i, guardrail));
    }

    // Role Context section
    out.push('\n');
    out.push_str("## Role Context\n");
    out.push('\n');
    out.push_str(params.prompt_md_content.trim_end());
    out.push('\n');

    // Reference section
    out.push('\n');
    out.push_str("## Reference: Operation Mode\n");
    out.push('\n');
    out.push_str(
        "When running autonomously inside Ralph Orchestrator, you follow the\n\
         operational workflows described in: ",
    );
    out.push_str(params.reference_dir);
    out.push('\n');
    out.push_str("These do not apply in interactive mode — the human drives the workflow.\n");

    out
}

/// Injects GitHub App credentials into the current process environment.
///
/// When `credential_dir` is provided, looks for `hosts.yml` at
/// `<credential_dir>/gh/hosts.yml` (D-02 shared credential path) and sets
/// `GH_CONFIG_DIR` to `<credential_dir>/gh`.
///
/// Falls back to the legacy workspace path `<ws_path>/.config/gh/hosts.yml`
/// when `credential_dir` is `None` (deprecated — will be removed).
///
/// Returns `true` if credentials were found and injected, `false` otherwise.
pub fn inject_app_credentials(
    ws_path: &Path,
    credential_dir: Option<&Path>,
    team_name: &str,
    member_name: &str,
) -> bool {
    if let Some(cred_dir) = credential_dir {
        return inject_app_credentials_from_shared_dir(cred_dir);
    }

    let gh_dir = ws_path.join(".config/gh");
    let hosts_yml = gh_dir.join("hosts.yml");

    if hosts_yml.exists() {
        refresh_token_from_keyring(ws_path, team_name, member_name);
        std::env::set_var("GH_CONFIG_DIR", &gh_dir);
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        eprintln!("Using GitHub App identity (GH_CONFIG_DIR: {})", gh_dir.display());
        true
    } else if gh_dir.is_dir() {
        eprintln!("Warning: App credential directory found but hosts.yml is missing. Using personal GitHub auth.");
        false
    } else {
        eprintln!("No App credentials found. Using personal GitHub auth. Run 'bm start' first to provision App credentials.");
        false
    }
}

/// Injects GitHub App credentials from an explicit shared credential directory.
///
/// `credential_dir` is the member-specific credential directory; the function
/// looks for `hosts.yml` at `<credential_dir>/gh/hosts.yml` and sets
/// `GH_CONFIG_DIR` to `<credential_dir>/gh`.
///
/// Returns `true` if credentials were found and injected, `false` otherwise.
pub(crate) fn inject_app_credentials_from_shared_dir(credential_dir: &Path) -> bool {
    let gh_dir = credential_dir.join("gh");
    if !gh_dir.join("hosts.yml").exists() {
        return false;
    }
    std::env::set_var("GH_CONFIG_DIR", &gh_dir);
    true
}

/// Injects App credentials for an agent launch session.
///
/// When `shared_credential_dir` is `Some`, uses the D-02 shared credential path.
/// Falls back to the legacy `ws_path/.config/gh` path when `None`.
///
/// Returns `true` if credentials were found and injected into the process environment.
pub(crate) fn prepare_launch_credentials(
    ws_path: &Path,
    shared_credential_dir: Option<&Path>,
    team_name: &str,
    member_name: &str,
) -> bool {
    inject_app_credentials(ws_path, shared_credential_dir, team_name, member_name)
}

/// One-shot token refresh: reads App credentials from the keyring, generates
/// a fresh JWT, exchanges it for an installation token, and writes it to
/// hosts.yml. Failures are logged as warnings — the caller continues with
/// whatever token is already on disk.
fn refresh_token_from_keyring(ws_path: &Path, team_name: &str, member_name: &str) {
    use crate::formation::{self, CredentialDomain};
    use crate::git::{app_auth, manifest_flow::credential_keys};

    let formation = match formation::local::create_local_formation(team_name) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Warning: could not create formation for token refresh: {e}");
            return;
        }
    };

    let store = match formation.credential_store(CredentialDomain::GitHubApp {
        team_name: team_name.to_string(),
        member_name: member_name.to_string(),
    }) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Warning: could not open credential store: {e}");
            return;
        }
    };

    let client_id = match store.retrieve(&credential_keys::client_id(member_name)) {
        Ok(Some(v)) => v,
        _ => return,
    };
    let private_key = match store.retrieve(&credential_keys::private_key(member_name)) {
        Ok(Some(v)) => v,
        _ => return,
    };
    let installation_id: u64 = match store.retrieve(&credential_keys::installation_id(member_name)) {
        Ok(Some(v)) => match v.parse() {
            Ok(id) => id,
            Err(_) => return,
        },
        _ => return,
    };

    let jwt = match app_auth::generate_jwt(&client_id, &private_key) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("Warning: JWT generation failed during token refresh: {e}");
            return;
        }
    };

    let inst_token = match app_auth::exchange_for_installation_token(&jwt, installation_id) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Warning: token exchange failed during token refresh: {e}");
            return;
        }
    };

    if let Err(e) = formation.refresh_token(member_name, ws_path, &inst_token.token) {
        eprintln!("Warning: failed to write refreshed token: {e}");
    }
}

/// Resolves and injects GitHub App credentials for a session launch.
///
/// Uses `session.credential_dir` (D-02 shared path) when present;
/// falls back to the legacy workspace `.config/gh` lookup when absent.
pub(crate) fn setup_launch_credentials(
    session: &AgentSession,
    team_name: &str,
    member_name: &str,
) -> bool {
    prepare_launch_credentials(&session.ws_path, session.credential_dir.as_deref(), team_name, member_name)
}

/// Launches a chat session by writing the meta-prompt to a temp file,
/// resolving the coding agent, and spawning it as a child process.
///
/// Returns the agent's exit code. The caller is responsible for
/// propagating it (e.g., via `std::process::exit`).
///
/// If `initial_prompt` is provided, it is piped to the coding agent's
/// stdin as the first turn via `(echo "<prompt>"; cat) | <binary> ...`.
/// This triggers the agent to act on the prompt immediately while keeping
/// the session interactive (cat reconnects stdin to the terminal).
pub fn launch_session(
    session: &AgentSession,
    team: &crate::config::TeamEntry,
    team_repo: &Path,
    member_name: &str,
    initial_prompt: Option<&str>,
    autonomous: bool,
) -> Result<i32> {
    let manifest = crate::profile::read_team_repo_manifest(team_repo)?;
    let coding_agent = crate::profile::resolve_coding_agent(team, &manifest)?;

    setup_launch_credentials(session, &team.name, member_name);

    let mut tmp_file = tempfile::Builder::new()
        .prefix("bm-session-")
        .suffix(".md")
        .tempfile()
        .context("Failed to create temp file for meta-prompt")?;
    tmp_file
        .write_all(session.meta_prompt.as_bytes())
        .context("Failed to write meta-prompt to temp file")?;

    let tmp_path = tmp_file.into_temp_path();
    let tmp_path_str = tmp_path
        .to_str()
        .context("Temp path is not valid UTF-8")?;
    let prompt_flag = coding_agent.system_prompt_flag.as_deref().with_context(|| {
        format!(
            "Coding agent '{}' ({}) does not define a system_prompt_flag",
            coding_agent.display_name, coding_agent.binary
        )
    })?;

    let mut args: Vec<String> = vec![
        prompt_flag.to_string(),
        tmp_path_str.to_string(),
    ];
    if autonomous {
        if let Some(flag) = coding_agent.skip_permissions_flag.as_deref() {
            args.push(flag.to_string());
        }
    }
    if let Some(prompt) = initial_prompt {
        args.push(prompt.to_string());
    }

    let spawn_config = spawn::SpawnConfig {
        agent_binary: coding_agent.binary.clone(),
        agent_args: args,
        working_dir: session.ws_path.clone(),
        env_vars: vec![],
    };

    let result = spawn::spawn_and_wait(&spawn_config)?;
    Ok(result.exit_code)
}

/// Resolves a member name from a role. Scans the team repo's `members/`
/// directory for a member whose role matches the requested one.
/// When multiple members share a role, returns the first alphabetically.
pub fn resolve_member_by_role(team_repo: &Path, role: &str) -> Result<String> {
    let members_dir = team_repo.join("members");
    if !members_dir.is_dir() {
        bail!("No members directory found in team repo");
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&members_dir)
        .context("Failed to read members directory")?
        .filter_map(|e| e.ok())
    {
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        let (member_role, _) = read_member_info(&entry.path(), &name)?;
        if member_role == role {
            candidates.push(name);
        }
    }

    match candidates.len() {
        0 => bail!(
            "No member with role '{}' found. Run `bm members list` to see hired members.",
            role
        ),
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => {
            candidates.sort();
            Ok(candidates.into_iter().next().unwrap())
        }
    }
}

/// Prepares a meeting session from an ephemeral workspace path provided by the daemon.
/// The meeting's `instructions` field IS the system prompt — no meta-prompt assembly.
pub fn prepare_meeting_session_from_path(
    workspace_path: &Path,
    instructions: &str,
) -> Result<AgentSession> {
    if instructions.trim().is_empty() {
        bail!("Meeting instructions must not be empty");
    }
    Ok(AgentSession {
        meta_prompt: instructions.to_string(),
        ws_path: workspace_path.to_path_buf(),
        credential_dir: None,
    })
}

/// Combines a meeting's initial prompt with user-provided trailing input.
///
/// Examples:
/// - prompt="start", input="plan the auth feature" → "start plan the auth feature"
/// - prompt="start", input=None → "start"
/// - prompt=None, input="plan something" → "plan something"
/// - prompt=None, input=None → None
pub fn build_meeting_prompt(
    prompt: Option<&str>,
    user_input: Option<&str>,
) -> Option<String> {
    match (prompt, user_input) {
        (Some(p), Some(input)) => Some(format!("{} {}", p, input)),
        (Some(p), None) => Some(p.to_string()),
        (None, Some(input)) => Some(input.to_string()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params() -> (Vec<String>, BTreeMap<String, String>, String) {
        let guardrails = vec![
            "Always follow team invariants".to_string(),
            "Use gh CLI for all GitHub operations".to_string(),
        ];
        let mut hats = BTreeMap::new();
        hats.insert(
            "executor".to_string(),
            "You are the executor hat.\nPick up tasks and execute them.\n".to_string(),
        );
        hats.insert(
            "reviewer".to_string(),
            "You are the reviewer hat.\nReview code for quality.\n".to_string(),
        );
        let prompt_md = "# Objective\n\nHandle team management tasks.\n".to_string();
        (guardrails, hats, prompt_md)
    }

    #[test]
    fn meta_prompt_contains_role_identity() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            result.contains("# Interactive Session — bob"),
            "Missing header"
        );
        assert!(
            result.contains("You are a member of the my-team team."),
            "Missing team identity"
        );
        assert!(result.contains("Your name is bob."), "Missing name");
        assert!(
            result.contains("Your role is called chief-of-staff."),
            "Missing role"
        );
    }

    #[test]
    fn guardrails_included_with_numbering() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "alice",
            role_name: "architect",
            role_description: "Test role description",
            team_name: "dev-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(result.contains("## Guardrails"), "Missing Guardrails heading");
        assert!(
            result.contains("999. Always follow team invariants"),
            "Missing guardrail 999"
        );
        assert!(
            result.contains("1000. Use gh CLI for all GitHub operations"),
            "Missing guardrail 1000"
        );
    }

    #[test]
    fn prompt_md_content_in_role_context() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            result.contains("## Role Context"),
            "Missing Role Context heading"
        );
        assert!(
            result.contains("Handle team management tasks"),
            "Missing PROMPT.md content"
        );
    }

    #[test]
    fn hatless_mode_includes_all_hats() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            result.contains("### executor"),
            "Missing executor hat heading"
        );
        assert!(
            result.contains("### reviewer"),
            "Missing reviewer hat heading"
        );
        assert!(
            result.contains("Pick up tasks and execute them"),
            "Missing executor instructions"
        );
        assert!(
            result.contains("Review code for quality"),
            "Missing reviewer instructions"
        );
    }

    #[test]
    fn hat_specific_mode_includes_only_one_hat() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: Some("executor"),
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            result.contains("Pick up tasks and execute them"),
            "Missing executor instructions"
        );
        assert!(
            !result.contains("Review code for quality"),
            "Reviewer instructions should not appear in executor-only mode"
        );
    }

    #[test]
    fn reference_materials_are_paths_not_inlined() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            result.contains("## Reference: Operation Mode"),
            "Missing Reference heading"
        );
        assert!(
            result.contains("ralph-prompts/reference/"),
            "Missing reference path"
        );
        assert!(
            result.contains("These do not apply in interactive mode"),
            "Missing interactive mode note"
        );
    }

    #[test]
    fn output_is_well_formed_markdown() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);

        // Verify the meta-prompt's own heading starts with H1
        let lines: Vec<&str> = result.lines().collect();
        assert!(
            lines[0].starts_with("# Interactive Session"),
            "First line should be the H1 header"
        );

        // Verify the four structural H2 sections exist
        let h2_lines: Vec<&&str> = lines
            .iter()
            .filter(|l| l.starts_with("## ") && !l.starts_with("### "))
            .collect();
        let h2_texts: Vec<&str> = h2_lines.iter().map(|l| l.trim()).collect();
        assert!(h2_texts.contains(&"## Your Capabilities"));
        assert!(h2_texts.contains(&"## Guardrails"));
        assert!(h2_texts.contains(&"## Role Context"));
        assert!(h2_texts.contains(&"## Reference: Operation Mode"));
    }

    #[test]
    fn empty_guardrails_produces_empty_section() {
        let hats = BTreeMap::new();
        let params = MetaPromptParams {
            member_name: "x",
            role_name: "r",
            role_description: "",
            team_name: "t",
            guardrails: &[],
            hat_instructions: &hats,
            prompt_md_content: "",
            reference_dir: "ref/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        // Guardrails heading should exist even if empty
        assert!(result.contains("## Guardrails"));
        assert!(!result.contains("999."), "No numbered items when guardrails empty");
    }

    #[test]
    fn hat_specific_with_unknown_hat_produces_empty_capabilities() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: Some("nonexistent"),
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        // Should still produce valid output, just with empty capabilities
        assert!(result.contains("## Your Capabilities"));
        assert!(!result.contains("executor"));
        assert!(!result.contains("reviewer"));
    }

    #[test]
    fn meta_prompt_includes_role_description() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "superman",
            role_description: "All-in-one member -- PO, architect, dev, QE, SRE, content writer",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            result.contains("All-in-one member -- PO, architect, dev, QE, SRE, content writer"),
            "Missing role description in identity section"
        );
    }

    #[test]
    fn meta_prompt_empty_role_description_no_blank_line() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "superman",
            role_description: "",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        // Role line should end with period and go straight to autonomy line
        assert!(
            result.contains("Your role is called superman.\nYou normally run autonomously"),
            "Empty role_description should not insert extra text after role name"
        );
    }

    #[test]
    fn skills_table_rendered_when_present() {
        let (guardrails, hats, prompt_md) = sample_params();
        let skills = vec![
            SkillInfo {
                name: "gh".to_string(),
                description: "Manages GitHub Projects v2 workflows".to_string(),
                load_command: "team/coding-agent/skills/gh/SKILL.md".to_string(),
            },
            SkillInfo {
                name: "status-workflow".to_string(),
                description: "Performs status transitions".to_string(),
                load_command: "team/coding-agent/skills/status-workflow/SKILL.md".to_string(),
            },
        ];
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &skills,
        };
        let result = build_meta_prompt(&params);

        // Verify Skills heading
        assert!(result.contains("## Skills"), "Missing Skills heading");
        // Verify table header
        assert!(
            result.contains("| Skill | Description | Load Command |"),
            "Missing table header"
        );
        // Verify both rows
        assert!(
            result.contains("| gh | Manages GitHub Projects v2 workflows | Read `team/coding-agent/skills/gh/SKILL.md` |"),
            "Missing gh skill row"
        );
        assert!(
            result.contains("| status-workflow | Performs status transitions | Read `team/coding-agent/skills/status-workflow/SKILL.md` |"),
            "Missing status-workflow skill row"
        );
        // Verify footer
        assert!(
            result.contains("To use a skill, read its SKILL.md file for full instructions."),
            "Missing skills footer"
        );

        // Verify Skills section appears between Capabilities and Guardrails
        let caps_pos = result.find("## Your Capabilities").unwrap();
        let skills_pos = result.find("## Skills").unwrap();
        let guard_pos = result.find("## Guardrails").unwrap();
        assert!(
            caps_pos < skills_pos && skills_pos < guard_pos,
            "Skills should appear between Capabilities and Guardrails"
        );

        // With skills, should have 5 H2 sections
        let lines: Vec<&str> = result.lines().collect();
        let h2_count = lines
            .iter()
            .filter(|l| l.starts_with("## ") && !l.starts_with("### "))
            .count();
        assert_eq!(h2_count, 5, "Should have 5 H2 sections when skills present");
    }

    #[test]
    fn skills_section_omitted_when_empty() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            !result.contains("## Skills"),
            "Skills heading should not appear when skills list is empty"
        );
    }

    #[test]
    fn interactive_mode_framing_present() {
        let (guardrails, hats, prompt_md) = sample_params();
        let params = MetaPromptParams {
            member_name: "bob",
            role_name: "chief-of-staff",
            role_description: "Test role description",
            team_name: "my-team",
            guardrails: &guardrails,
            hat_instructions: &hats,
            prompt_md_content: &prompt_md,
            reference_dir: "ralph-prompts/reference/",
            hat: None,
            skills: &[],
        };
        let result = build_meta_prompt(&params);
        assert!(
            result.contains("Right now you are in an interactive session with the human (PO)"),
            "Missing interactive mode framing"
        );
        assert!(
            result.contains("You normally run autonomously inside Ralph Orchestrator"),
            "Missing autonomy context"
        );
    }

    #[test]
    fn resolve_member_by_role_finds_member() {
        let tmp = tempfile::tempdir().unwrap();
        let members = tmp.path().join("members");
        let member_dir = members.join("engineer-alice");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            member_dir.join("botminter.yml"),
            "role: engineer\nname: alice\n",
        )
        .unwrap();

        let result = resolve_member_by_role(tmp.path(), "engineer").unwrap();
        assert_eq!(result, "engineer-alice");
    }

    #[test]
    fn resolve_member_by_role_no_match() {
        let tmp = tempfile::tempdir().unwrap();
        let members = tmp.path().join("members");
        let member_dir = members.join("engineer-alice");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            member_dir.join("botminter.yml"),
            "role: engineer\nname: alice\n",
        )
        .unwrap();

        let result = resolve_member_by_role(tmp.path(), "architect");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("architect"));
    }

    #[test]
    fn resolve_member_by_role_infers_from_name() {
        let tmp = tempfile::tempdir().unwrap();
        let members = tmp.path().join("members");
        let member_dir = members.join("engineer-bob");
        std::fs::create_dir_all(&member_dir).unwrap();

        let result = resolve_member_by_role(tmp.path(), "engineer").unwrap();
        assert_eq!(result, "engineer-bob");
    }

    #[test]
    fn resolve_member_by_role_picks_first_alphabetically() {
        let tmp = tempfile::tempdir().unwrap();
        let members = tmp.path().join("members");
        for name in ["engineer-charlie", "engineer-alice", "engineer-bob"] {
            std::fs::create_dir_all(members.join(name)).unwrap();
        }

        let result = resolve_member_by_role(tmp.path(), "engineer").unwrap();
        assert_eq!(result, "engineer-alice");
    }

    #[test]
    fn prepare_meeting_session_from_path_empty_instructions_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let result = prepare_meeting_session_from_path(tmp.path(), "   ");
        let err = result.err().expect("should fail for empty instructions");
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn prepare_meeting_session_from_path_returns_valid_session() {
        let tmp = tempfile::tempdir().unwrap();
        let session = prepare_meeting_session_from_path(tmp.path(), "You are an engineer.")
            .expect("should succeed with any path");
        assert_eq!(session.meta_prompt, "You are an engineer.");
        assert_eq!(session.ws_path, tmp.path());
    }

    #[test]
    fn prepare_chat_session_from_path_sets_credential_dir() {
        let tmp = tempfile::tempdir().unwrap();

        // Team repo: members/<member> dir + botminter.yml for manifest
        let team_repo = tmp.path().join("team");
        let member_dir = team_repo.join("members").join("engineer-alice");
        std::fs::create_dir_all(&member_dir).unwrap();
        std::fs::write(
            team_repo.join("botminter.yml"),
            "name: test\ndisplay_name: Test\ndescription: d\nversion: 1.0.0\nschema_version: \"1\"\nroles: []\n",
        )
        .unwrap();

        // Workspace: minimal ralph.yml + PROMPT.md
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("ralph.yml"), "hats: {}\n").unwrap();
        std::fs::write(workspace.join("PROMPT.md"), "# Objective\n").unwrap();

        let session = prepare_chat_session_from_path(
            &team_repo,
            "test-team",
            "engineer-alice",
            &workspace,
            None,
        )
        .expect("session should be created");

        let cred_dir = session
            .credential_dir
            .expect("credential_dir must be Some, not None");
        let cred_str = cred_dir.to_str().unwrap();
        assert!(
            cred_str.ends_with("sessions/test-team/credentials/engineer-alice"),
            "credential_dir must be sessions_base/credentials/<member>, got: {cred_str}"
        );
    }

    // inject_app_credentials tests — serialized via mutex because they
    // manipulate process-global env vars (GH_TOKEN, GITHUB_TOKEN, GH_CONFIG_DIR).
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn inject_app_credentials_sets_gh_config_dir_when_hosts_yml_present() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let gh_dir = tmp.path().join(".config/gh");
        std::fs::create_dir_all(&gh_dir).unwrap();
        std::fs::write(gh_dir.join("hosts.yml"), "github.com:\n  oauth_token: test\n").unwrap();

        std::env::remove_var("GH_CONFIG_DIR");

        let result = inject_app_credentials(tmp.path(), None, "test-team", "test-member");

        assert!(result, "Should return true when credentials are available");
        let config_dir =
            std::env::var("GH_CONFIG_DIR").expect("GH_CONFIG_DIR should be set");
        assert_eq!(
            config_dir,
            gh_dir.to_str().unwrap(),
            "GH_CONFIG_DIR should point to .config/gh/"
        );

        std::env::remove_var("GH_CONFIG_DIR");
    }

    #[test]
    fn inject_app_credentials_removes_conflicting_tokens() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let gh_dir = tmp.path().join(".config/gh");
        std::fs::create_dir_all(&gh_dir).unwrap();
        std::fs::write(gh_dir.join("hosts.yml"), "github.com:\n  oauth_token: test\n").unwrap();

        std::env::set_var("GH_TOKEN", "should-be-removed");
        std::env::set_var("GITHUB_TOKEN", "should-be-removed");

        let result = inject_app_credentials(tmp.path(), None, "test-team", "test-member");

        assert!(result, "Should return true when credentials are available");
        assert!(
            std::env::var("GH_TOKEN").is_err(),
            "GH_TOKEN should be removed"
        );
        assert!(
            std::env::var("GITHUB_TOKEN").is_err(),
            "GITHUB_TOKEN should be removed"
        );

        std::env::remove_var("GH_CONFIG_DIR");
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
    }

    #[test]
    fn inject_app_credentials_noop_when_no_config_dir() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();

        std::env::set_var("GH_TOKEN", "preserved");
        std::env::set_var("GITHUB_TOKEN", "preserved");
        std::env::remove_var("GH_CONFIG_DIR");

        let result = inject_app_credentials(tmp.path(), None, "test-team", "test-member");

        assert!(!result, "Should return false when no credentials directory");
        assert!(
            std::env::var("GH_CONFIG_DIR").is_err(),
            "GH_CONFIG_DIR should not be set"
        );
        assert_eq!(
            std::env::var("GH_TOKEN").unwrap(),
            "preserved",
            "GH_TOKEN should be preserved"
        );
        assert_eq!(
            std::env::var("GITHUB_TOKEN").unwrap(),
            "preserved",
            "GITHUB_TOKEN should be preserved"
        );

        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
    }

    #[test]
    fn inject_app_credentials_noop_when_hosts_yml_missing() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let gh_dir = tmp.path().join(".config/gh");
        std::fs::create_dir_all(&gh_dir).unwrap();

        std::env::remove_var("GH_CONFIG_DIR");

        let result = inject_app_credentials(tmp.path(), None, "test-team", "test-member");

        assert!(!result, "Should return false when hosts.yml is missing");
        assert!(
            std::env::var("GH_CONFIG_DIR").is_err(),
            "GH_CONFIG_DIR should not be set when hosts.yml is missing"
        );

        std::env::remove_var("GH_CONFIG_DIR");
    }

    #[test]
    fn inject_app_credentials_sets_gh_config_dir_to_shared_credential_dir() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        // hosts.yml lives at <credential_dir>/gh/hosts.yml (D-02 shared path)
        let shared_gh_dir = tmp.path().join("gh");
        std::fs::create_dir_all(&shared_gh_dir).unwrap();
        std::fs::write(
            shared_gh_dir.join("hosts.yml"),
            "github.com:\n  oauth_token: shared_token\n",
        )
        .unwrap();

        std::env::remove_var("GH_CONFIG_DIR");

        let result = inject_app_credentials_from_shared_dir(tmp.path());

        assert!(
            result,
            "inject_app_credentials_from_shared_dir must return true when \
             hosts.yml exists at <credential_dir>/gh/hosts.yml"
        );
        let config_dir =
            std::env::var("GH_CONFIG_DIR").expect("GH_CONFIG_DIR must be set after injection");
        assert_eq!(
            config_dir,
            shared_gh_dir.to_str().unwrap(),
            "GH_CONFIG_DIR must point to the shared credential gh/ subdir, \
             not workspace/.config/gh/"
        );

        std::env::remove_var("GH_CONFIG_DIR");
    }

    // ── CT-154-03: inject_app_credentials must use D-02 shared path ─────────

    #[test]
    fn inject_app_credentials_uses_d02_credential_dir_over_workspace_config_gh() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();

        // hosts.yml exists ONLY at the D-02 shared path, NOT at ws/.config/gh
        let credential_dir = tmp.path().join("credentials").join("alice");
        let shared_gh_dir = credential_dir.join("gh");
        std::fs::create_dir_all(&shared_gh_dir).unwrap();
        std::fs::write(
            shared_gh_dir.join("hosts.yml"),
            "github.com:\n    oauth_token: shared_token\n",
        )
        .unwrap();

        std::env::remove_var("GH_CONFIG_DIR");

        // credential_dir is provided; ws_path has no .config/gh/hosts.yml
        let result = inject_app_credentials(tmp.path(), Some(&credential_dir), "team", "alice");

        assert!(
            result,
            "inject_app_credentials must return true when credential_dir/gh/hosts.yml exists, \
             even when ws_path/.config/gh/hosts.yml does not"
        );
        let config_dir =
            std::env::var("GH_CONFIG_DIR").expect("GH_CONFIG_DIR must be set after injection");
        assert_eq!(
            config_dir,
            shared_gh_dir.to_str().unwrap(),
            "GH_CONFIG_DIR must point to D-02 shared path <credential_dir>/gh, \
             not workspace/.config/gh"
        );

        std::env::remove_var("GH_CONFIG_DIR");
    }

    #[test]
    fn prepare_launch_credentials_uses_shared_credential_dir_when_provided() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();

        let ws_path = tmp.path().join("workspace");
        std::fs::create_dir_all(&ws_path).unwrap();
        // hosts.yml only at the D-02 shared path — NOT at ws_path/.config/gh
        let shared_cred_dir = tmp.path().join("credentials").join("alice");
        let shared_gh_dir = shared_cred_dir.join("gh");
        std::fs::create_dir_all(&shared_gh_dir).unwrap();
        std::fs::write(
            shared_gh_dir.join("hosts.yml"),
            "github.com:\n    oauth_token: shared_token\n",
        )
        .unwrap();

        std::env::remove_var("GH_CONFIG_DIR");

        let injected =
            prepare_launch_credentials(&ws_path, Some(&shared_cred_dir), "team", "alice");

        assert!(
            injected,
            "prepare_launch_credentials must return true when shared_credential_dir/gh/hosts.yml \
             exists, even when ws_path/.config/gh does not"
        );
        let config_dir = std::env::var("GH_CONFIG_DIR").expect(
            "GH_CONFIG_DIR must be set to shared credential path after prepare_launch_credentials \
             with Some(shared_credential_dir)",
        );
        assert_eq!(
            config_dir,
            shared_gh_dir.to_str().unwrap(),
            "GH_CONFIG_DIR must point to shared credential dir/gh, not ws_path/.config/gh"
        );

        std::env::remove_var("GH_CONFIG_DIR");
    }

    #[test]
    fn setup_launch_credentials_uses_credential_dir_not_workspace_path() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();

        // D-02 shared path: sessions_base/credentials/<member> (no /gh suffix)
        let credential_dir = tmp.path().join("credentials").join("alice");
        let credential_gh_dir = credential_dir.join("gh");
        std::fs::create_dir_all(&credential_gh_dir).unwrap();
        std::fs::write(
            credential_gh_dir.join("hosts.yml"),
            "github.com:\n    oauth_token: shared_token\n",
        )
        .unwrap();

        // ws_path has NO .config/gh/hosts.yml — only the shared credential dir has it
        let ws_path = tmp.path().join("ws");
        std::fs::create_dir_all(&ws_path).unwrap();

        let session = AgentSession {
            meta_prompt: "test".to_string(),
            ws_path,
            credential_dir: Some(credential_dir.clone()),
        };

        std::env::remove_var("GH_CONFIG_DIR");

        let injected = setup_launch_credentials(&session, "team", "alice");

        assert!(
            injected,
            "setup_launch_credentials must return true when credential_dir/gh/hosts.yml exists \
             (session.credential_dir = Some), even when ws_path/.config/gh does not"
        );
        let config_dir = std::env::var("GH_CONFIG_DIR").expect(
            "GH_CONFIG_DIR must be set to D-02 shared credential path when \
             session.credential_dir is Some",
        );
        assert_eq!(
            config_dir,
            credential_gh_dir.to_str().unwrap(),
            "GH_CONFIG_DIR must point to credential_dir/gh (D-02 shared path), \
             not ws_path/.config/gh (deprecated permanent workspace path)"
        );

        std::env::remove_var("GH_CONFIG_DIR");
    }

    #[test]
    fn setup_launch_credentials_credential_dir_excludes_gh_suffix_gh_appended_internally() {
        let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();

        // credential_dir = sessions_base/credentials/<member> — NO /gh suffix
        // The /gh suffix is appended internally by inject_app_credentials_from_shared_dir
        let credential_dir = tmp.path().join("sessions").join("credentials").join("bob");
        let credential_gh_dir = credential_dir.join("gh");
        std::fs::create_dir_all(&credential_gh_dir).unwrap();
        std::fs::write(
            credential_gh_dir.join("hosts.yml"),
            "github.com:\n    oauth_token: bob_token\n",
        )
        .unwrap();

        let ws_path = tmp.path().join("ws");
        std::fs::create_dir_all(&ws_path).unwrap();

        let session = AgentSession {
            meta_prompt: "test".to_string(),
            ws_path,
            credential_dir: Some(credential_dir.clone()),
        };

        std::env::remove_var("GH_CONFIG_DIR");

        let injected = setup_launch_credentials(&session, "team", "bob");
        assert!(
            injected,
            "setup_launch_credentials must return true when credential_dir is \
             sessions_base/credentials/bob and credential_dir/gh/hosts.yml exists"
        );

        let config_dir = std::env::var("GH_CONFIG_DIR").expect("GH_CONFIG_DIR must be set");
        assert_eq!(
            config_dir,
            credential_gh_dir.to_str().unwrap(),
            "GH_CONFIG_DIR must equal credential_dir/gh — /gh is appended internally, \
             credential_dir itself must not contain /gh"
        );

        std::env::remove_var("GH_CONFIG_DIR");
    }
}
