use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::config::{self, BotminterConfig, Credentials, TeamEntry};
use crate::profile;

/// Whether to create a new GitHub Project board or use an existing one.
enum ProjectChoice {
    CreateNew,
    UseExisting(u64),
}

/// Runs the `bm init` interactive wizard.
pub fn run() -> Result<()> {
    // Prerequisite checks
    check_prerequisites()?;

    // --- Wizard ---
    cliclack::intro("botminter — create a new team")?;

    // Workzone location
    let default_workzone = default_workzone_path();
    let workzone: String = cliclack::input("Where should teams live? (workzone directory)")
        .default_input(&default_workzone.to_string_lossy())
        .interact()?;
    let workzone = expand_tilde(&workzone);

    // Team name
    let team_name: String = cliclack::input("Team name")
        .placeholder("my-team")
        .validate(|input: &String| {
            if input.is_empty() {
                Err("Team name cannot be empty")
            } else if input.contains('/') || input.contains(' ') {
                Err("Team name cannot contain '/' or spaces")
            } else {
                Ok(())
            }
        })
        .interact()?;

    // Check idempotency: fail if team dir exists
    let team_dir = workzone.join(&team_name);
    if team_dir.exists() {
        bail!(
            "Directory '{}' already exists. Choose a different team name. \
             If this is from a failed init, delete the directory and retry.",
            team_dir.display()
        );
    }

    // Select profile
    let profiles = profile::list_profiles();
    let profile_options: Vec<(String, String, String)> = profiles
        .iter()
        .map(|name| {
            let manifest = profile::read_manifest(name).unwrap();
            (
                name.clone(),
                manifest.display_name.clone(),
                manifest.description.clone(),
            )
        })
        .collect();

    let profile_items: Vec<(&str, &str, &str)> = profile_options
        .iter()
        .map(|(v, l, h)| (v.as_str(), l.as_str(), h.as_str()))
        .collect();

    let selected_profile: String = cliclack::select("Which profile?")
        .items(&profile_items)
        .interact()
        .map(|s: &str| s.to_string())?;

    // GitHub integration — detect token, browse orgs/repos
    let token = detect_or_prompt_gh_token()?;
    validate_gh_token(&token)?;

    let github_org = select_github_org(&token)?;
    let (github_repo, is_new_repo) = select_or_create_repo(&token, &github_org, &team_name)?;

    // Select or create a GitHub Project board
    let github_owner = github_repo.split('/').next().unwrap_or(&github_org);
    let project_choice = select_or_create_project(&token, github_owner, &team_name)?;

    let gh_token = Some(token);

    let telegram_token: String = cliclack::input("Telegram bot token (optional, enter to skip)")
        .default_input("")
        .required(false)
        .interact()?;
    let telegram_bot_token = if telegram_token.is_empty() {
        None
    } else {
        Some(telegram_token)
    };

    let manifest = profile::read_manifest(&selected_profile)?;

    // Hire members and add projects (only for new repos — existing repos already have content)
    let (members_to_hire, projects_to_add) = if is_new_repo {
        let role_names: Vec<String> = manifest.roles.iter().map(|r| r.name.clone()).collect();
        let members = collect_members(&role_names)?;
        let projects = collect_projects(gh_token.as_deref(), Some(&github_org))?;
        (members, projects)
    } else {
        cliclack::log::info(
            "Existing repo selected — use `bm hire` and `bm projects add` to modify the team after init.",
        )?;
        (Vec::new(), Vec::new())
    };

    // Summary
    let mut summary = format!(
        "Team: {}\nProfile: {}\nWorkzone: {}",
        team_name,
        selected_profile,
        workzone.display()
    );
    summary.push_str(&format!("\nGitHub: {}", github_repo));
    match &project_choice {
        ProjectChoice::CreateNew => {
            summary.push_str(&format!("\nProject board: new ({} Board)", team_name));
        }
        ProjectChoice::UseExisting(n) => {
            summary.push_str(&format!("\nProject board: existing (#{n})"));
        }
    }
    if !members_to_hire.is_empty() {
        summary.push_str("\nMembers:");
        for (role, name) in &members_to_hire {
            summary.push_str(&format!("\n  {}-{}", role, name));
        }
    }
    if !projects_to_add.is_empty() {
        summary.push_str("\nProjects:");
        for (name, url) in &projects_to_add {
            summary.push_str(&format!("\n  {} ({})", name, url));
        }
    }

    cliclack::log::info(summary)?;

    let confirm: bool = cliclack::confirm("Create this team?").interact()?;
    if !confirm {
        cliclack::outro("Aborted.")?;
        return Ok(());
    }

    // --- Execution ---
    let spinner = cliclack::spinner();
    spinner.start("Creating team...");

    // 1. Create workzone + team dirs
    fs::create_dir_all(&workzone)
        .with_context(|| format!("Failed to create workzone at {}", workzone.display()))?;
    fs::create_dir_all(&team_dir)
        .with_context(|| format!("Failed to create team directory at {}", team_dir.display()))?;

    // 2. Set up team repo (new: init + extract + push, existing: clone)
    let team_repo = team_dir.join("team");

    if is_new_repo {
        fs::create_dir_all(&team_repo).context("Failed to create team repo directory")?;

        spinner.start("Initializing git repository...");
        run_git(&team_repo, &["init", "-b", "main"])?;

        spinner.start("Extracting profile content...");
        profile::extract_profile_to(&selected_profile, &team_repo)?;

        if !projects_to_add.is_empty() {
            augment_manifest_with_projects(&team_repo, &projects_to_add)?;
        }

        fs::create_dir_all(team_repo.join("team")).context("Failed to create team/ dir")?;
        fs::create_dir_all(team_repo.join("projects")).context("Failed to create projects/ dir")?;
        fs::write(team_repo.join("team/.gitkeep"), "").ok();
        fs::write(team_repo.join("projects/.gitkeep"), "").ok();

        for (role, name) in &members_to_hire {
            let member_dir_name = format!("{}-{}", role, name);
            let member_dir = team_repo.join("team").join(&member_dir_name);
            fs::create_dir_all(&member_dir)
                .with_context(|| format!("Failed to create member dir {}", member_dir.display()))?;
            profile::extract_member_to(&selected_profile, role, &member_dir)?;
            finalize_member_manifest(&member_dir, name)?;
        }

        for (proj_name, _url) in &projects_to_add {
            let proj_dir = team_repo.join("projects").join(proj_name);
            fs::create_dir_all(proj_dir.join("knowledge"))
                .with_context(|| format!("Failed to create projects/{}/knowledge/", proj_name))?;
            fs::create_dir_all(proj_dir.join("invariants"))
                .with_context(|| format!("Failed to create projects/{}/invariants/", proj_name))?;
            fs::write(proj_dir.join("knowledge/.gitkeep"), "").ok();
            fs::write(proj_dir.join("invariants/.gitkeep"), "").ok();
        }

        spinner.start("Creating initial commit...");
        run_git(&team_repo, &["add", "-A"])?;
        let commit_msg = format!("feat: initialize team repo ({} profile)", selected_profile);
        run_git(&team_repo, &["commit", "-m", &commit_msg])?;

        spinner.start("Creating GitHub repository...");
        create_github_repo(&team_repo, &github_repo, gh_token.as_deref())?;
    } else {
        spinner.start("Cloning existing repository...");
        clone_existing_repo(&team_dir, &github_repo, gh_token.as_deref())?;
    }

    // 3. Register in config (early — before GitHub metadata ops so a failure
    //    in labels/project doesn't leave ~/.botminter in a broken state)
    spinner.start("Registering team...");
    let mut cfg = load_or_default_config();

    let team_entry = TeamEntry {
        name: team_name.clone(),
        path: team_dir.clone(),
        profile: selected_profile.clone(),
        github_repo: github_repo.clone(),
        credentials: Credentials {
            gh_token: gh_token.clone(),
            telegram_bot_token: telegram_bot_token.clone(),
            webhook_secret: None,
        },
    };
    cfg.teams.push(team_entry);

    if cfg.teams.len() == 1 {
        cfg.default_team = Some(team_name.clone());
    }
    cfg.workzone = workzone.clone();

    config::save(&cfg)?;

    // 4. Bootstrap labels (idempotent via --force)
    spinner.start("Bootstrapping labels...");
    if let Err(e) = bootstrap_labels(&github_repo, &manifest.labels, gh_token.as_deref()) {
        spinner.stop("Label bootstrap failed");
        let label_cmds: Vec<String> = manifest
            .labels
            .iter()
            .map(|l| {
                format!(
                    "gh label create '{}' --color '{}' --description '{}' --force --repo {}",
                    l.name, l.color, l.description, github_repo,
                )
            })
            .collect();
        bail!(
            "Failed to bootstrap labels: {}\n\n\
             To fix, run these commands manually:\n  {}\n\n\
             Make sure your token has Issues (Write) permission.",
            e,
            label_cmds.join("\n  "),
        );
    }

    // 5. Create or sync GitHub Project board
    let owner = github_repo.split('/').next().unwrap_or(&github_repo);
    let project_number = match project_choice {
        ProjectChoice::CreateNew => {
            spinner.start("Creating GitHub Project board...");
            match create_github_project(owner, &team_name, &manifest.statuses, gh_token.as_deref())
            {
                Ok(n) => {
                    spinner.stop("GitHub Project board created");
                    n
                }
                Err(e) => {
                    spinner.stop("Project creation failed");
                    bail!(
                        "Failed to create GitHub Project: {}\n\n\
                         To fix, create the project manually and then run:\n  \
                         gh project create --owner {} --title '{} Board'\n  \
                         bm projects sync\n\n\
                         Make sure your token has the \"project\" scope (classic PAT) \
                         or \"Organization projects: Admin\" (fine-grained PAT).",
                        e, owner, team_name,
                    );
                }
            }
        }
        ProjectChoice::UseExisting(n) => {
            spinner.start("Syncing project board statuses...");
            sync_project_status_field(owner, n, &manifest.statuses, gh_token.as_deref())?;
            spinner.stop("Project board statuses synced");
            n
        }
    };

    // Print view setup instructions if the profile defines views
    if !manifest.views.is_empty() {
        let project_url = format!(
            "https://github.com/orgs/{}/projects/{}",
            owner, project_number
        );
        cliclack::log::info(format!(
            "Set up role-based views at: {}\n\
             Run `bm projects sync` anytime to see view instructions.",
            project_url,
        ))?;
    }

    spinner.stop("Done!");
    cliclack::outro(format!(
        "Team '{}' created at {}",
        team_name,
        team_dir.display()
    ))?;

    Ok(())
}

/// Checks that `git` is available. Errors if not found.
fn check_prerequisites() -> Result<()> {
    let mut missing = Vec::new();
    if which::which("git").is_err() {
        missing.push("git — https://git-scm.com/");
    }
    if which::which("gh").is_err() {
        missing.push("gh — https://cli.github.com/");
    }
    if !missing.is_empty() {
        bail!(
            "Missing required tools:\n  {}\n\nInstall them and try again.",
            missing.join("\n  "),
        );
    }
    Ok(())
}

/// Default workzone path: ~/.botminter/workspaces
fn default_workzone_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".botminter")
        .join("workspaces")
}

/// Expands `~` at the start of a path to the home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

/// Run a git command in the given directory.
pub(crate) fn run_git(dir: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        );
    }
    Ok(())
}

/// Collect members to hire during init (optional).
fn collect_members(roles: &[String]) -> Result<Vec<(String, String)>> {
    let hire_members: bool = cliclack::confirm("Hire members now?")
        .initial_value(false)
        .interact()?;

    if !hire_members {
        return Ok(Vec::new());
    }

    let mut members = Vec::new();
    loop {
        let role_items: Vec<(&str, &str, &str)> = roles
            .iter()
            .map(|r| (r.as_str(), r.as_str(), ""))
            .collect();

        let role: String = cliclack::select("Select role")
            .items(&role_items)
            .interact()
            .map(|s: &str| s.to_string())?;

        let name: String = cliclack::input("Member name")
            .placeholder("bob")
            .validate(|input: &String| {
                if input.is_empty() {
                    Err("Name cannot be empty")
                } else if input.contains('/') || input.contains(' ') {
                    Err("Name cannot contain '/' or spaces")
                } else {
                    Ok(())
                }
            })
            .interact()?;

        members.push((role, name));

        let more: bool = cliclack::confirm("Hire another member?")
            .initial_value(false)
            .interact()?;
        if !more {
            break;
        }
    }

    Ok(members)
}

/// Collect projects to add during init (optional).
/// When `gh_token` and `org` are provided, offers interactive repo selection.
fn collect_projects(
    gh_token: Option<&str>,
    org: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let add_projects: bool = cliclack::confirm("Add projects now?")
        .initial_value(false)
        .interact()?;

    if !add_projects {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    loop {
        let url = if let (Some(token), Some(org)) = (gh_token, org) {
            select_project_repo(token, org)?
        } else {
            prompt_project_url()?
        };

        let name = derive_project_name(&url);
        cliclack::log::info(format!("Project name: {}", name))?;

        projects.push((name, url));

        let more: bool = cliclack::confirm("Add another project?")
            .initial_value(false)
            .interact()?;
        if !more {
            break;
        }
    }

    Ok(projects)
}

/// Prompts for a project fork URL manually (fallback when no token/org).
fn prompt_project_url() -> Result<String> {
    let url: String = cliclack::input("Project fork URL (must be HTTPS)")
        .placeholder("https://github.com/org/repo.git")
        .validate(|input: &String| {
            if input.is_empty() {
                Err("URL cannot be empty")
            } else if !input.starts_with("https://") {
                Err("URL must be HTTPS (e.g. https://github.com/org/repo.git)")
            } else {
                Ok(())
            }
        })
        .interact()?;
    Ok(url)
}

/// Derives the project name from a git URL (basename minus .git suffix).
pub fn derive_project_name(url: &str) -> String {
    let url = url.trim_end_matches('/');
    let basename = url.rsplit('/').next().unwrap_or(url);
    basename.trim_end_matches(".git").to_string()
}

/// Augments the botminter.yml in the team repo with a projects section.
fn augment_manifest_with_projects(
    team_repo: &Path,
    projects: &[(String, String)],
) -> Result<()> {
    let manifest_path = team_repo.join("botminter.yml");
    let mut manifest: profile::ProfileManifest = {
        let contents = fs::read_to_string(&manifest_path)
            .context("Failed to read botminter.yml from team repo")?;
        serde_yml::from_str(&contents).context("Failed to parse botminter.yml")?
    };

    manifest.projects = projects
        .iter()
        .map(|(name, url)| profile::ProjectDef {
            name: name.clone(),
            fork_url: url.clone(),
        })
        .collect();

    let contents = serde_yml::to_string(&manifest)
        .context("Failed to serialize augmented botminter.yml")?;
    fs::write(&manifest_path, contents)
        .context("Failed to write augmented botminter.yml")?;

    Ok(())
}

/// Reads the .botminter.yml template, augments with the member name, and
/// writes as botminter.yml (without the dot prefix).
pub(crate) fn finalize_member_manifest(member_dir: &Path, name: &str) -> Result<()> {
    let template_path = member_dir.join(".botminter.yml");
    if template_path.exists() {
        let contents = fs::read_to_string(&template_path)
            .context("Failed to read .botminter.yml template")?;

        // Parse, augment with name, write back
        let mut value: serde_yml::Value =
            serde_yml::from_str(&contents).context("Failed to parse .botminter.yml")?;

        if let serde_yml::Value::Mapping(ref mut map) = value {
            map.insert(
                serde_yml::Value::String("name".to_string()),
                serde_yml::Value::String(name.to_string()),
            );
        }

        let augmented =
            serde_yml::to_string(&value).context("Failed to serialize member manifest")?;

        // Write as botminter.yml (no dot prefix)
        let manifest_path = member_dir.join("botminter.yml");
        fs::write(&manifest_path, augmented)
            .context("Failed to write member botminter.yml")?;

        // Remove the template (.botminter.yml)
        fs::remove_file(&template_path).ok();
    }

    Ok(())
}

/// Creates a GitHub repo and pushes the team repo.
fn create_github_repo(team_repo: &Path, repo_name: &str, gh_token: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "repo",
        "create",
        repo_name,
        "--private",
        "--source",
        ".",
        "--push",
    ])
    .current_dir(team_repo);

    if let Some(token) = gh_token {
        cmd.env("GH_TOKEN", token);
    }

    let output = cmd.output().context("Failed to run `gh repo create`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "gh repo create failed: {}\n\n\
             To fix, run manually:\n  \
             gh repo create {} --private --source . --push",
            stderr.trim(),
            repo_name,
        );
    }

    Ok(())
}

/// Clones an existing GitHub repo into `{team_dir}/team/`.
pub fn clone_existing_repo(
    team_dir: &Path,
    repo_name: &str,
    gh_token: Option<&str>,
) -> Result<()> {
    let target = team_dir.join("team");
    let mut cmd = Command::new("gh");
    cmd.args([
        "repo",
        "clone",
        repo_name,
        &target.to_string_lossy(),
    ]);

    if let Some(token) = gh_token {
        cmd.env("GH_TOKEN", token);
    }

    let output = cmd.output().context("Failed to run `gh repo clone`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Failed to clone repo '{}': {}\n\n\
             To fix, run manually:\n  \
             gh repo clone {} {}",
            repo_name,
            stderr.trim(),
            repo_name,
            target.display(),
        );
    }

    Ok(())
}

/// Prompts the user to enter a GitHub token manually.
fn prompt_gh_token() -> Result<String> {
    let token: String = cliclack::input("GitHub token (GH_TOKEN)")
        .placeholder("ghp_... or github_pat_...")
        .validate(|input: &String| {
            if input.is_empty() {
                Err("GitHub token is required when a GitHub repo is specified")
            } else {
                Ok(())
            }
        })
        .interact()?;
    Ok(token)
}

/// Masks a token for display: shows first 4 and last 4 characters.
fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        return "****".to_string();
    }
    format!("{}...{}", &token[..4], &token[token.len() - 4..])
}

/// Validates that a GitHub token works by calling `gh api user`.
fn validate_gh_token(token: &str) -> Result<()> {
    let output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .env("GH_TOKEN", token)
        .output()
        .context("Failed to run `gh api user` for token validation")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "GitHub token validation failed: {}\n\n\
             Make sure your token is valid and not expired.\n\
             To create a new token, visit: https://github.com/settings/tokens\n\
             Required permissions: Contents (Write), Issues (Write), Projects (Admin)",
            stderr.trim(),
        );
    }

    let login = String::from_utf8_lossy(&output.stdout).trim().to_string();
    cliclack::log::info(format!("Authenticated as: {}", login))?;
    Ok(())
}

/// Detects an existing GH_TOKEN or prompts the user for one.
fn detect_or_prompt_gh_token() -> Result<String> {
    let detected = std::env::var("GH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            Command::new("gh")
                .args(["auth", "token"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|t| !t.is_empty())
        });

    if let Some(existing) = detected {
        let masked = mask_token(&existing);
        let use_existing: bool =
            cliclack::confirm(format!("GitHub token detected ({}). Use it?", masked))
                .initial_value(true)
                .interact()?;
        if use_existing {
            return Ok(existing);
        }
    }
    prompt_gh_token()
}

/// Lists the user's GitHub orgs + personal account via `gh api`, returns selected org login.
/// Falls back to manual input if no orgs are discovered (common with fine-grained PATs
/// that lack the Organization: Read permission).
fn select_github_org(gh_token: &str) -> Result<String> {
    // Get the authenticated user's login
    let user_output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .env("GH_TOKEN", gh_token)
        .output()
        .context("Failed to get GitHub user")?;
    let user_login = String::from_utf8_lossy(&user_output.stdout).trim().to_string();

    // Try to discover orgs (may return empty if token lacks org:read scope)
    let org_output = Command::new("gh")
        .args(["api", "user/orgs", "--jq", ".[].login"])
        .env("GH_TOKEN", gh_token)
        .output()
        .context("Failed to list GitHub orgs")?;
    let org_stdout = String::from_utf8_lossy(&org_output.stdout);
    let orgs: Vec<String> = org_stdout.lines().filter(|l| !l.is_empty()).map(String::from).collect();

    // Build selection: personal account + discovered orgs + manual entry option
    let personal_label = format!("{} (personal)", user_login);
    let mut select_items: Vec<(String, String, String)> = vec![
        (user_login.clone(), personal_label, "Your personal GitHub account".to_string()),
    ];
    for org in &orgs {
        select_items.push((org.clone(), org.clone(), "Organization".to_string()));
    }
    select_items.push((
        "__other__".to_string(),
        "Other (type org name)".to_string(),
        "Enter an org name not listed above".to_string(),
    ));

    let items_ref: Vec<(&str, &str, &str)> = select_items
        .iter()
        .map(|(v, l, d)| (v.as_str(), l.as_str(), d.as_str()))
        .collect();

    let selected: &str = cliclack::select("GitHub owner (type to filter)")
        .items(&items_ref)
        .filter_mode()
        .interact()?;

    if selected == "__other__" {
        let org: String = cliclack::input("Organization name")
            .placeholder("my-org")
            .validate(|input: &String| {
                if input.is_empty() {
                    Err("Organization name cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact()?;
        Ok(org)
    } else {
        Ok(selected.to_string())
    }
}

/// Lists repos for an org/user and lets the user select one or create a new one.
/// Returns `(owner/repo, is_new)` — `is_new` is true when the user chose to create a new repo.
fn select_or_create_repo(gh_token: &str, owner: &str, team_name: &str) -> Result<(String, bool)> {
    let repos = list_gh_repos(gh_token, owner)?;

    let default_name = format!("{}-team", team_name);
    let create_label = format!("Create new repo ({})", default_name);

    let mut select_items: Vec<(String, String, String)> = vec![
        ("__create__".to_string(), create_label.clone(), "Create a new private repository".to_string()),
    ];
    for repo in &repos {
        select_items.push((repo.clone(), repo.clone(), String::new()));
    }

    let items_ref: Vec<(&str, &str, &str)> = select_items
        .iter()
        .map(|(v, l, d)| (v.as_str(), l.as_str(), d.as_str()))
        .collect();

    let selected: &str = cliclack::select("Team repo (type to filter)")
        .items(&items_ref)
        .filter_mode()
        .interact()?;

    if selected == "__create__" {
        let repo_name: String = cliclack::input("New repo name")
            .default_input(&default_name)
            .interact()?;
        Ok((format!("{}/{}", owner, repo_name), true))
    } else {
        Ok((format!("{}/{}", owner, selected), false))
    }
}

/// Lists GitHub Project boards for an owner and lets the user select one or create a new one.
/// Returns `ProjectChoice::CreateNew` or `ProjectChoice::UseExisting(number)`.
fn select_or_create_project(
    gh_token: &str,
    owner: &str,
    team_name: &str,
) -> Result<ProjectChoice> {
    let projects = list_gh_projects(gh_token, owner)?;

    let default_title = format!("{} Board", team_name);
    let create_label = format!("Create new board ({})", default_title);

    let mut select_items: Vec<(String, String, String)> = vec![(
        "__create__".to_string(),
        create_label,
        "Create a new GitHub Project board".to_string(),
    )];
    for (number, title) in &projects {
        select_items.push((
            number.to_string(),
            format!("{} (#{number})", title),
            String::new(),
        ));
    }

    let items_ref: Vec<(&str, &str, &str)> = select_items
        .iter()
        .map(|(v, l, d)| (v.as_str(), l.as_str(), d.as_str()))
        .collect();

    let selected: &str = cliclack::select("Project board (type to filter)")
        .items(&items_ref)
        .filter_mode()
        .interact()?;

    if selected == "__create__" {
        Ok(ProjectChoice::CreateNew)
    } else {
        let number: u64 = selected
            .parse()
            .context("Failed to parse project number")?;
        Ok(ProjectChoice::UseExisting(number))
    }
}

/// Lists GitHub Project boards for a given owner. Returns `(number, title)` pairs.
pub fn list_gh_projects(gh_token: &str, owner: &str) -> Result<Vec<(u64, String)>> {
    let output = Command::new("gh")
        .args([
            "project",
            "list",
            "--owner",
            owner,
            "--format",
            "json",
        ])
        .env("GH_TOKEN", gh_token)
        .output()
        .context("Failed to run `gh project list`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Failed to list projects for '{}': {}",
            owner,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(stdout.trim()).context("Could not parse project list JSON")?;

    Ok(json["projects"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|p| {
            let number = p["number"].as_u64()?;
            let title = p["title"].as_str()?.to_string();
            Some((number, title))
        })
        .collect())
}

/// Lists repos for an org/user and lets the user select one as a project fork.
/// Returns the HTTPS clone URL.
fn select_project_repo(gh_token: &str, org: &str) -> Result<String> {
    let repos = list_gh_repos(gh_token, org)?;

    if repos.is_empty() {
        cliclack::log::warning(format!("No repos found in '{}'. Enter URL manually.", org))?;
        return prompt_project_url();
    }

    let items_ref: Vec<(&str, &str, &str)> = repos
        .iter()
        .map(|r| (r.as_str(), r.as_str(), ""))
        .collect();

    let selected: &str = cliclack::select("Select project repo (type to filter)")
        .items(&items_ref)
        .filter_mode()
        .interact()?;

    Ok(format!("https://github.com/{}/{}.git", org, selected))
}

/// Lists repository names for a given GitHub owner (org or user).
fn list_gh_repos(gh_token: &str, owner: &str) -> Result<Vec<String>> {
    let output = Command::new("gh")
        .args([
            "repo", "list", owner,
            "--limit", "50",
            "--json", "name",
            "--jq", ".[].name",
        ])
        .env("GH_TOKEN", gh_token)
        .output()
        .context("Failed to list repos")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to list repos for '{}': {}", owner, stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().filter(|l| !l.is_empty()).map(String::from).collect())
}

/// Bootstraps labels on the GitHub repo from the profile manifest.
fn bootstrap_labels(
    repo: &str,
    labels: &[profile::LabelDef],
    gh_token: Option<&str>,
) -> Result<()> {
    for label in labels {
        let mut cmd = Command::new("gh");
        cmd.args([
            "label",
            "create",
            &label.name,
            "--color",
            &label.color,
            "--description",
            &label.description,
            "--force",
            "--repo",
            repo,
        ]);

        if let Some(token) = gh_token {
            cmd.env("GH_TOKEN", token);
        }

        let output = cmd.output().with_context(|| {
            format!("Failed to create label '{}'", label.name)
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "Failed to create label '{}': {}",
                label.name,
                stderr.trim(),
            );
        }
    }
    Ok(())
}

/// Creates a GitHub Project (v2), syncs the Status field options, and returns the project number.
/// Uses the `updateProjectV2Field` GraphQL mutation to replace the built-in
/// Status field's default options with the profile's status definitions.
fn create_github_project(
    owner: &str,
    team_name: &str,
    statuses: &[profile::StatusDef],
    gh_token: Option<&str>,
) -> Result<u64> {
    // 1. Create project
    let mut cmd = Command::new("gh");
    cmd.args([
        "project",
        "create",
        "--owner",
        owner,
        "--title",
        &format!("{} Board", team_name),
        "--format",
        "json",
    ]);
    if let Some(token) = gh_token {
        cmd.env("GH_TOKEN", token);
    }
    let output = cmd
        .output()
        .context("Failed to run `gh project create`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh project create failed: {}", stderr.trim());
    }

    // Parse project number from JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let project_json: serde_json::Value = serde_json::from_str(stdout.trim())
        .context("Could not parse JSON from `gh project create` output")?;
    let project_number = project_json["number"]
        .as_u64()
        .context("Could not find 'number' field in gh project create output")?;

    // 2. Sync Status field options
    sync_project_status_field(owner, project_number, statuses, gh_token)?;

    Ok(project_number)
}

/// Finds the built-in Status field ID and updates its options via GraphQL.
/// This replaces the default (Todo/In Progress/Done) with profile-defined statuses.
pub fn sync_project_status_field(
    owner: &str,
    project_number: u64,
    statuses: &[profile::StatusDef],
    gh_token: Option<&str>,
) -> Result<()> {
    let num_str = project_number.to_string();

    // 1. Find the Status field ID
    let mut cmd = Command::new("gh");
    cmd.args([
        "project",
        "field-list",
        &num_str,
        "--owner",
        owner,
        "--format",
        "json",
    ]);
    if let Some(token) = gh_token {
        cmd.env("GH_TOKEN", token);
    }
    let output = cmd
        .output()
        .context("Failed to run `gh project field-list`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh project field-list failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let fields_json: serde_json::Value = serde_json::from_str(stdout.trim())
        .context("Could not parse field-list JSON")?;

    let field_id = fields_json["fields"]
        .as_array()
        .and_then(|fields| {
            fields
                .iter()
                .find(|f| f["name"].as_str() == Some("Status"))
                .and_then(|f| f["id"].as_str())
        })
        .context("Could not find Status field in project")?
        .to_string();

    // 2. Build the GraphQL mutation to update Status field options
    //    Assign colors by role prefix for visual grouping.
    let options_json: Vec<String> = statuses
        .iter()
        .map(|s| {
            let color = color_for_status(&s.name);
            format!(
                "{{name:\"{}\",color:{},description:\"\"}}",
                s.name, color
            )
        })
        .collect();

    let mutation = format!(
        "mutation {{ updateProjectV2Field(input: {{ fieldId: \"{}\", \
         singleSelectOptions: [{}] }}) {{ projectV2Field {{ \
         ... on ProjectV2SingleSelectField {{ name options {{ name id }} }} }} }} }}",
        field_id,
        options_json.join(",")
    );

    let mut cmd = Command::new("gh");
    cmd.args(["api", "graphql", "-f", &format!("query={}", mutation)]);
    if let Some(token) = gh_token {
        cmd.env("GH_TOKEN", token);
    }
    let output = cmd
        .output()
        .context("Failed to run GraphQL updateProjectV2Field")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to sync Status field: {}", stderr.trim());
    }

    Ok(())
}

/// Maps a status name prefix to a GitHub Project color for visual grouping.
fn color_for_status(name: &str) -> &'static str {
    match name.split(':').next().unwrap_or("") {
        "po" => "BLUE",
        "arch" => "PURPLE",
        "dev" => "YELLOW",
        "qe" => "PINK",
        "lead" => "ORANGE",
        "sre" => "GRAY",
        "cw" => "ORANGE",
        "error" => "RED",
        "done" => "GREEN",
        _ => "GRAY",
    }
}

/// Finds a GitHub Project by title for the given owner. Returns the project number.
pub fn find_project_number(
    owner: &str,
    team_name: &str,
    gh_token: Option<&str>,
) -> Result<u64> {
    let board_title = format!("{} Board", team_name);
    let mut cmd = Command::new("gh");
    cmd.args([
        "project",
        "list",
        "--owner",
        owner,
        "--format",
        "json",
    ]);
    if let Some(token) = gh_token {
        cmd.env("GH_TOKEN", token);
    }
    let output = cmd
        .output()
        .context("Failed to run `gh project list`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh project list failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(stdout.trim()).context("Could not parse project list JSON")?;

    json["projects"]
        .as_array()
        .and_then(|projects| {
            projects
                .iter()
                .find(|p| p["title"].as_str() == Some(&board_title))
                .and_then(|p| p["number"].as_u64())
        })
        .with_context(|| format!("No project named '{}' found for owner '{}'", board_title, owner))
}

/// Loads the existing config or returns a fresh default.
fn load_or_default_config() -> BotminterConfig {
    config::load().unwrap_or_else(|_| BotminterConfig {
        workzone: default_workzone_path(),
        default_team: None,
        teams: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── check_prerequisites ───────────────────────────────────

    #[test]
    fn check_prerequisites_passes_when_tools_present() {
        // Both git and gh are available in the test environment
        assert!(check_prerequisites().is_ok());
    }

    // ── mask_token ──────────────────────────────────────────────

    #[test]
    fn mask_token_normal() {
        assert_eq!(mask_token("github_pat_abc123xyz789"), "gith...z789");
    }

    #[test]
    fn mask_token_short_token_returns_stars() {
        assert_eq!(mask_token("abc"), "****");
    }

    #[test]
    fn mask_token_exactly_12_returns_stars() {
        assert_eq!(mask_token("123456789012"), "****");
    }

    #[test]
    fn mask_token_13_chars_shows_ends() {
        assert_eq!(mask_token("1234567890123"), "1234...0123");
    }

    // ── project number JSON parsing ─────────────────────────────

    #[test]
    fn parse_project_number_from_json() {
        let json_output = r#"{"number":42,"title":"test Board","url":"https://github.com/orgs/test/projects/42"}"#;
        let parsed: serde_json::Value = serde_json::from_str(json_output).unwrap();
        let number = parsed["number"].as_u64().unwrap();
        assert_eq!(number, 42);
    }

    #[test]
    fn parse_project_number_missing_field() {
        let json_output = r#"{"title":"test Board"}"#;
        let parsed: serde_json::Value = serde_json::from_str(json_output).unwrap();
        assert!(parsed["number"].as_u64().is_none());
    }

    // ── list_gh_repos parsing ───────────────────────────────────

    #[test]
    fn parse_repo_names_from_gh_output() {
        let output = "repo-one\nrepo-two\nrepo-three\n";
        let repos: Vec<String> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        assert_eq!(repos, vec!["repo-one", "repo-two", "repo-three"]);
    }

    #[test]
    fn parse_repo_names_empty_output() {
        let output = "";
        let repos: Vec<String> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        assert!(repos.is_empty());
    }

    // ── project URL construction ────────────────────────────────

    #[test]
    fn project_url_constructed_correctly() {
        let org = "my-org";
        let repo = "my-repo";
        let url = format!("https://github.com/{}/{}.git", org, repo);
        assert_eq!(url, "https://github.com/my-org/my-repo.git");
        assert_eq!(derive_project_name(&url), "my-repo");
    }
}
