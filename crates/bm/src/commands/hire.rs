use std::fs;
use std::io::IsTerminal as _;

use anyhow::{Context, Result};

use crate::bridge::{self, CredentialStore};
use crate::config;
use crate::git::manifest_flow;
use crate::member_lifecycle::{self, AppCredentials, HireParams};

/// GitHub App credential flags from the CLI.
pub struct AppCredentialFlags<'a> {
    pub reuse_app: bool,
    pub app_id: Option<&'a str>,
    pub client_id: Option<&'a str>,
    pub private_key_file: Option<&'a str>,
    pub installation_id: Option<&'a str>,
    pub save_credentials: Option<&'a str>,
}

/// Handles `bm hire <role> [--name <name>] [-t team] [--reuse-app ...]`.
pub fn run(
    role: &str,
    name: Option<&str>,
    team_flag: Option<&str>,
    app_flags: AppCredentialFlags<'_>,
) -> Result<()> {
    super::ensure_profiles(false)?;
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    // Resolve App credentials from CLI flags (if --reuse-app)
    let app_credentials = if app_flags.reuse_app {
        let app_id = app_flags.app_id.context("--reuse-app requires --app-id")?;
        let client_id = app_flags.client_id.context("--reuse-app requires --client-id")?;
        let key_file = app_flags.private_key_file.context("--reuse-app requires --private-key-file")?;
        let installation_id = app_flags.installation_id.context("--reuse-app requires --installation-id")?;
        let private_key = fs::read_to_string(key_file)
            .with_context(|| format!("Failed to read private key file: {key_file}"))?;
        Some(AppCredentials {
            app_id: app_id.to_string(),
            client_id: client_id.to_string(),
            private_key,
            installation_id: installation_id.to_string(),
        })
    } else {
        None
    };

    // ── Domain call ───────────────────────────────────────────────
    let result = member_lifecycle::hire_member(&HireParams {
        team,
        role,
        name,
        app_credentials,
        save_credentials_path: app_flags.save_credentials,
    })?;

    // ── Display ───────────────────────────────────────────────────
    if result.already_existed {
        println!(
            "Member {} already exists in team '{}'.",
            result.member_dir_name, team.name
        );
    } else {
        println!("Hired {} as {} in team '{}'.", role, result.member_name, team.name);
    }

    if result.app_credentials_stored {
        println!("GitHub App credentials stored for {}.", result.member_dir_name);
    } else if result.already_existed && !team.github_repo.is_empty() && !app_flags.reuse_app {
        // Member exists but --reuse-app not provided: fail fast instead of starting OAuth flow
        anyhow::bail!(
            "Member '{}' already exists in team '{}'. \
             Use --reuse-app to update its GitHub App credentials.",
            result.member_dir_name, team.name
        );
    } else if !result.already_existed && !team.github_repo.is_empty() && !app_flags.reuse_app {
        // New member without --reuse-app: run the interactive manifest flow
        run_manifest_flow_for_member(team, &result.member_dir_name, app_flags.save_credentials)?;
    }

    if let Some(path) = &result.credentials_saved_to {
        println!("Credentials saved to {path}");
    }

    // Prompt for bridge token if team has an external bridge configured
    prompt_bridge_token(&team.path.join("team"), team, &cfg, &result.member_dir_name)?;

    Ok(())
}

/// Runs the interactive manifest flow to create a GitHub App for a member,
/// then stores the resulting credentials.
fn run_manifest_flow_for_member(
    team: &config::TeamEntry,
    member_name: &str,
    save_credentials: Option<&str>,
) -> Result<()> {
    let org = manifest_flow::resolve_org_from_repo(&team.github_repo)?;
    let app_name = format!("{}-{}", team.name, member_name);
    let slug = manifest_flow::app_name_to_slug(&app_name);

    // Check for name collision before starting the browser flow
    match manifest_flow::check_name_collision(&slug) {
        Ok(true) => {
            anyhow::bail!(
                "A GitHub App named '{}' already exists.\n\
                 Try hiring with a different --name, or use --reuse-app to attach an existing App.",
                slug,
            );
        }
        Ok(false) => {}
        Err(e) => {
            eprintln!("Warning: could not check App name availability: {e}");
        }
    }

    // ── Context and instructions ─────────────────────────────────
    let is_tty = std::io::stdin().is_terminal();

    if is_tty {
        cliclack::log::info(format!(
            "GitHub App Setup for {member_name}\n\
             \n\
             Each team member gets its own GitHub App identity.\n\
             This creates '{app_name}[bot]' — a distinct bot account\n\
             for managing issues, PRs, and project boards.\n\
             \n\
             Two clicks needed:\n\
               1. Create the App on GitHub\n\
               2. Install it on your organization"
        ))?;
    }

    let browser_available = if is_tty {
        cliclack::confirm("Is a browser available on this machine?")
            .initial_value(true)
            .interact()?
    } else {
        // Non-interactive (piped stdin) — use browser path, BM_NO_BROWSER controls opening
        true
    };

    let team_repo_url = format!("https://github.com/{}", team.github_repo);

    let mut server = manifest_flow::prepare_manifest_flow(&manifest_flow::ManifestFlowParams {
        app_name: app_name.clone(),
        org,
        team_repo_url,
        github_api_base: std::env::var("BM_GITHUB_API_BASE").ok(),
        github_web_base: std::env::var("BM_GITHUB_WEB_BASE").ok(),
    })?;

    let flow_result = if browser_available {
        // ── Browser path ─────────────────────────────────────────
        if is_tty {
            cliclack::log::info(format!(
                "Click this link to create the GitHub App:\n\
                 \n\
                   {}", server.start_url,
            ))?;
        } else {
            eprintln!("Click this link to create the GitHub App:");
            eprintln!("  {}\n", server.start_url);
        }
        server.run()?
    } else {
        // ── Headless path ────────────────────────────────────────
        server.open_browser = false;
        server.stdin_fallback = true;

        let port = server.start_url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|s| s.split('/').next())
            .unwrap_or("PORT");

        cliclack::log::info(format!(
            "Open this URL in any browser:\n\
             \n\
               {}\n\
             \n\
             After creating and installing the App, GitHub will redirect\n\
             your browser to a URL starting with:\n\
             \n\
               http://127.0.0.1:{port}/callback?code=...\n\
             \n\
             If the page doesn't load, that's expected — copy the full\n\
             URL from your browser's address bar and paste it here.",
            server.start_url,
        ))?;

        eprint!("  Paste redirect URL: ");
        server.run()?
    };

    eprintln!();
    cliclack::log::success("GitHub App created and installed successfully!")?;

    // Let the user confirm before proceeding — they may still be in the browser
    if is_tty {
        eprint!("  Press Enter to continue...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }

    // Store the credentials
    let creds = AppCredentials {
        app_id: flow_result.app_id,
        client_id: flow_result.client_id,
        private_key: flow_result.private_key,
        installation_id: flow_result.installation_id,
    };

    let setup_result = member_lifecycle::setup_app_credentials(
        team,
        member_name,
        &creds,
        save_credentials,
    )?;

    println!("GitHub App credentials stored for {}.", member_name);
    if let Some(path) = &setup_result.credentials_saved_to {
        println!("Credentials saved to {path}");
    }

    Ok(())
}

/// Prompts for bridge token if the team has an external bridge and stdin is a TTY.
fn prompt_bridge_token(
    team_repo: &std::path::Path,
    team: &config::TeamEntry,
    cfg: &config::BotminterConfig,
    member_name: &str,
) -> Result<()> {
    let bridge_dir = match bridge::discover(team_repo, &team.name)? {
        Some(d) => d,
        None => return Ok(()),
    };
    let bridge_manifest = match bridge::load_manifest(&bridge_dir) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    if bridge_manifest.spec.bridge_type != "external" || !std::io::stdin().is_terminal() {
        return Ok(());
    }

    let display_name = bridge_manifest
        .metadata
        .display_name
        .as_deref()
        .unwrap_or(&bridge_manifest.metadata.name);

    let token: String = cliclack::input(format!(
        "{} bot token for {} (optional, press Enter to skip)",
        display_name, member_name
    ))
    .default_input("")
    .interact()?;

    if token.is_empty() {
        println!(
            "No bridge token provided. Add later with: bm bridge identity add {}",
            member_name
        );
        return Ok(());
    }

    let workzone = team.path.parent().unwrap_or(&team.path);
    let state_path = bridge::state_path(workzone, &team.name);
    let cred_store = bridge::LocalCredentialStore::new(
        &team.name,
        &bridge_manifest.metadata.name,
        state_path,
    )
    .with_collection(cfg.keyring_collection.clone());

    match cred_store.store(member_name, &token) {
        Ok(()) => println!("Bridge token stored for {}.", member_name),
        Err(e) => eprintln!(
            "Warning: Could not store token in keyring: {}. \
             Set BM_BRIDGE_TOKEN_{} env var instead.",
            e,
            bridge::env_var_suffix_pub(member_name)
        ),
    }

    Ok(())
}
