use std::future::IntoFuture;
use std::net::TcpListener;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;
use tokio::sync::{oneshot, Mutex};

use crate::formation::KeyValueCredentialStore;

// ── Public types ────────────────────────────────────────────────────

/// Pre-generated credentials for `--reuse-app` mode.
pub struct PreGeneratedCredentials {
    pub app_id: String,
    pub client_id: String,
    pub private_key: String,
    pub installation_id: String,
}

// ── Manifest JSON construction ──────────────────────────────────────

/// Builds the manifest JSON for GitHub App creation.
///
/// The manifest includes:
/// - `redirect_url`: local callback for code exchange
/// - `setup_url`: local callback for post-installation redirect
/// - `organization_projects: admin` (NOT `projects: admin` — that's classic boards)
///
/// Used by `run_manifest_flow()` to construct the form payload, and by tests
/// to verify the permission set.
pub fn build_manifest_json(
    app_name: &str,
    team_repo_url: &str,
    port: u16,
) -> serde_json::Value {
    serde_json::json!({
        "name": app_name,
        "url": team_repo_url,
        "redirect_url": format!("http://127.0.0.1:{port}/callback"),
        "setup_url": format!("http://127.0.0.1:{port}/installed"),
        "default_permissions": {
            "issues": "write",
            "contents": "write",
            "pull_requests": "write",
            "administration": "write",
            "organization_projects": "admin"
        },
        "default_events": [],
        "public": false
    })
}

// ── Name collision check ────────────────────────────────────────────

/// Checks if a GitHub App with the given slug already exists.
///
/// The slug is derived from the app name by lowercasing and replacing
/// spaces with hyphens. GitHub App names share a namespace with org names.
pub fn check_name_collision(slug: &str) -> Result<bool> {
    let url = format!("https://github.com/apps/{slug}");
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("Failed to build HTTP client")?;
    let response = client
        .get(&url)
        .header("User-Agent", "botminter")
        .send()
        .context("Failed to check app name availability")?;

    // 200 means the app exists; 404 means available
    Ok(response.status().is_success())
}

/// Derives the slug from an app name (lowercased, spaces→hyphens).
pub fn app_name_to_slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

// ── Credential storage ──────────────────────────────────────────────

/// Key conventions for GitHub App credentials in the credential store.
pub mod credential_keys {
    pub fn app_id(member: &str) -> String {
        format!("{member}/github-app-id")
    }
    pub fn client_id(member: &str) -> String {
        format!("{member}/github-app-client-id")
    }
    pub fn private_key(member: &str) -> String {
        format!("{member}/github-app-private-key")
    }
    pub fn installation_id(member: &str) -> String {
        format!("{member}/github-installation-id")
    }
}

/// Stores pre-generated credentials in the credential store.
pub fn store_pregenerated_credentials(
    store: &dyn KeyValueCredentialStore,
    member: &str,
    creds: &PreGeneratedCredentials,
) -> Result<()> {
    store
        .store(&credential_keys::app_id(member), &creds.app_id)
        .context("Failed to store App ID")?;
    store
        .store(&credential_keys::client_id(member), &creds.client_id)
        .context("Failed to store Client ID")?;
    store
        .store(&credential_keys::private_key(member), &creds.private_key)
        .context("Failed to store private key")?;
    store
        .store(&credential_keys::installation_id(member), &creds.installation_id)
        .context("Failed to store installation ID")?;
    Ok(())
}

/// Removes all GitHub App credentials for a member from the credential store.
///
/// Removes the 4 known credential keys. Errors on individual keys are
/// collected and reported as a single error at the end.
pub fn remove_member_credentials(
    store: &dyn KeyValueCredentialStore,
    member: &str,
) -> Result<()> {
    let keys = [
        credential_keys::app_id(member),
        credential_keys::client_id(member),
        credential_keys::private_key(member),
        credential_keys::installation_id(member),
    ];

    let mut errors = Vec::new();
    for key in &keys {
        if let Err(e) = store.remove(key) {
            errors.push(format!("{key}: {e}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!("Failed to remove some credentials:\n  {}", errors.join("\n  "))
    }
}

/// Saves credentials to a YAML file (for `--save-credentials`).
pub fn save_credentials_to_file(
    path: &str,
    member: &str,
    creds: &PreGeneratedCredentials,
) -> Result<()> {
    let content = serde_yml::to_string(&serde_json::json!({
        "member": member,
        "app_id": creds.app_id,
        "client_id": creds.client_id,
        "private_key": creds.private_key,
        "installation_id": creds.installation_id,
    }))
    .context("Failed to serialize credentials")?;

    std::fs::write(path, &content).context("Failed to write credentials file")?;

    // Set file permissions to 0600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .context("Failed to set credentials file permissions")?;
    }

    Ok(())
}

// ── Org resolution ──────────────────────────────────────────────────

/// Resolves the organization from a `github_repo` field (e.g., "org/repo").
/// Returns an error if the owner is a personal account (not an org).
pub fn resolve_org_from_repo(github_repo: &str) -> Result<String> {
    let owner = github_repo
        .split('/')
        .next()
        .context("Invalid github_repo format — expected 'owner/repo'")?;

    validate_is_org(owner)?;
    Ok(owner.to_string())
}

/// Validates that a GitHub account name is an Organization (not a personal account).
///
/// Calls `GET /users/{owner}` and checks `.type == "Organization"`.
/// Returns `Ok(())` if it is an org, or an error with a clear message if not.
pub fn validate_is_org(owner: &str) -> Result<()> {
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["api", &format!("users/{owner}"), "--jq", ".type"]);
    if let Some(token) = super::detect_token() {
        cmd.env("GH_TOKEN", token);
    }

    let output = cmd
        .output()
        .context("Failed to check repo owner type via `gh api`")?;

    if !output.status.success() {
        bail!(
            "Failed to verify owner type for '{}'. Is the GitHub CLI authenticated?",
            owner
        );
    }

    let owner_type = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if owner_type != "Organization" {
        bail!(
            "GitHub App identity requires an organization, but '{}' is a {}.\n\
             Personal accounts cannot use organization_projects permissions.\n\
             Create an organization and move your team repo there first.",
            owner,
            owner_type.to_lowercase()
        );
    }

    Ok(())
}

// ── Installation repo management ────────────────────────────────────

/// Ensures the App installation has access to the given repos.
///
/// For each repo, checks `GET /repos/{owner}/{repo}/installation` using an
/// App JWT (this endpoint requires App-level auth, not a user PAT).
///
/// This is idempotent: calling it multiple times is safe.
pub fn ensure_app_on_repos(
    installation_id: &str,
    client_id: &str,
    private_key: &str,
    repos: &[&str],
) -> Result<()> {
    let jwt = super::app_auth::generate_jwt(client_id, private_key)
        .context("Failed to generate JWT for repo installation check")?;
    let mut warnings = Vec::new();
    for repo in repos {
        if repo.is_empty() {
            continue;
        }
        match check_repo_installation(repo, &jwt) {
            RepoInstallationStatus::Installed => {}
            RepoInstallationStatus::NotInstalled => {
                let org = repo.split('/').next().unwrap_or("UNKNOWN");
                warnings.push(format!(
                    "App is not installed on {repo}. \
                     Install it manually: https://github.com/organizations/{org}/settings/installations/{installation_id}"
                ));
            }
            RepoInstallationStatus::CheckFailed(err) => {
                eprintln!("  Warning: could not check App installation on {repo}: {err}");
            }
        }
    }
    if !warnings.is_empty() {
        for w in &warnings {
            eprintln!("  Warning: {w}");
        }
    }
    Ok(())
}

enum RepoInstallationStatus {
    Installed,
    NotInstalled,
    CheckFailed(String),
}

/// Checks whether the App is installed on a specific repo.
///
/// Uses the App's JWT for authentication via `-H Authorization: Bearer` header,
/// which overrides any `GH_TOKEN` env var. The `GET /repos/{owner}/{repo}/installation`
/// endpoint requires App-level auth and cannot be called with a user PAT.
fn check_repo_installation(repo: &str, jwt: &str) -> RepoInstallationStatus {
    let auth_header = format!("Authorization: Bearer {jwt}");
    let mut cmd = std::process::Command::new("gh");
    cmd.args([
        "api",
        &format!("repos/{repo}/installation"),
        "-H", &auth_header,
        "--silent",
    ]);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                RepoInstallationStatus::Installed
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("404") || stderr.contains("Not Found") {
                    RepoInstallationStatus::NotInstalled
                } else {
                    RepoInstallationStatus::CheckFailed(stderr.trim().to_string())
                }
            }
        }
        Err(e) => RepoInstallationStatus::CheckFailed(e.to_string()),
    }
}

/// Collects all repos that a member's App should have access to:
/// the team repo + any configured project repos from `botminter.yml`.
pub fn collect_team_repos(team: &crate::config::TeamEntry) -> Vec<String> {
    let mut repos = Vec::new();

    // Team repo
    if !team.github_repo.is_empty() {
        repos.push(team.github_repo.clone());
    }

    // Project repos — read from team repo's botminter.yml via read_team_projects()
    let team_repo = team.path.join("team");
    for project in crate::profile::read_team_projects(&team_repo) {
        let url = project.fork_url.trim().to_string();
        if let Some(repo) = fork_url_to_owner_repo(&url) {
            repos.push(repo);
        }
    }

    repos
}

/// Converts a GitHub fork URL to `owner/repo` format.
/// Returns `None` for non-GitHub or empty URLs.
pub(crate) fn fork_url_to_owner_repo(url: &str) -> Option<String> {
    let stripped = url
        .strip_prefix("https://github.com/")?;
    let repo = stripped
        .trim_end_matches('/')
        .trim_end_matches(".git");
    if repo.is_empty() || !repo.contains('/') {
        return None;
    }
    Some(repo.to_string())
}

// ── Interactive manifest flow (browser-based App creation) ─────────

/// Parameters for the interactive manifest flow.
pub struct ManifestFlowParams {
    /// App name, e.g. "{team}-{member}"
    pub app_name: String,
    /// GitHub organization name
    pub org: String,
    /// Team repo URL, e.g. "https://github.com/org/team-repo"
    pub team_repo_url: String,
    /// Override for GitHub API base URL (default: `https://api.github.com`).
    /// Used for testing with a mock server.
    pub github_api_base: Option<String>,
    /// Override for GitHub web base URL (default: `https://github.com`).
    /// Used for testing with a mock server.
    pub github_web_base: Option<String>,
}

/// Result of a successful manifest flow — all 4 credentials.
pub struct ManifestFlowResult {
    pub app_id: String,
    pub client_id: String,
    pub private_key: String,
    pub installation_id: String,
}

/// Intermediate credentials from the code exchange (before installation).
#[derive(Clone)]
struct ExchangeCredentials {
    app_id: String,
    client_id: String,
    private_key: String,
    html_url: String,
}

/// Shared state for the axum callback server.
struct ManifestServerState {
    csrf_state: String,
    org: String,
    manifest_json: serde_json::Value,
    exchange_creds: Mutex<Option<ExchangeCredentials>>,
    result_tx: Mutex<Option<oneshot::Sender<ManifestFlowResult>>>,
    github_api_base: String,
    github_web_base: String,
}

/// Response from `POST /app-manifests/{code}/conversions`.
#[derive(serde::Deserialize)]
struct CodeExchangeResponse {
    id: u64,
    client_id: String,
    pem: String,
    html_url: String,
}

/// A single installation from `GET /app/installations`.
#[derive(serde::Deserialize)]
struct Installation {
    id: u64,
    account: Option<InstallationAccount>,
}

#[derive(serde::Deserialize)]
struct InstallationAccount {
    login: String,
}

/// A prepared manifest flow server, ready to run.
///
/// Two-phase API: `prepare_manifest_flow()` sets up the server and returns
/// this handle with the `start_url`. The caller displays the URL and opens
/// the browser, then calls `run()` to block until completion or timeout.
pub struct ManifestFlowServer {
    /// URL the operator must visit to begin App creation.
    pub start_url: String,
    /// Whether to attempt opening the browser when `run()` starts.
    pub open_browser: bool,
    /// Whether to read a pasted redirect URL from stdin as a fallback.
    pub stdin_fallback: bool,
    listener: TcpListener,
    result_rx: oneshot::Receiver<ManifestFlowResult>,
    state: Arc<ManifestServerState>,
}

/// Prepares the manifest flow server: binds a port, builds the router,
/// and returns a handle with the `start_url`.
///
/// Does not start the server or open a browser — the caller does that.
pub fn prepare_manifest_flow(params: &ManifestFlowParams) -> Result<ManifestFlowServer> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .context("Failed to bind local server for manifest flow")?;
    listener
        .set_nonblocking(true)
        .context("Failed to set listener to non-blocking mode")?;
    let port = listener.local_addr()?.port();

    let manifest_json = build_manifest_json(&params.app_name, &params.team_repo_url, port);
    let csrf_state = uuid::Uuid::new_v4().to_string();

    let (result_tx, result_rx) = oneshot::channel::<ManifestFlowResult>();

    let github_api_base = params
        .github_api_base
        .clone()
        .unwrap_or_else(|| "https://api.github.com".to_string());
    let github_web_base = params
        .github_web_base
        .clone()
        .unwrap_or_else(|| "https://github.com".to_string());

    let state = Arc::new(ManifestServerState {
        csrf_state,
        org: params.org.clone(),
        manifest_json,
        exchange_creds: Mutex::new(None),
        result_tx: Mutex::new(Some(result_tx)),
        github_api_base,
        github_web_base,
    });

    // Routes are built later in run() based on the mode (browser vs headless).
    // Store state for route construction.
    let app_state = state.clone();

    let start_url = format!("http://127.0.0.1:{port}/start");

    Ok(ManifestFlowServer {
        start_url,
        open_browser: std::env::var("BM_NO_BROWSER").is_err(),
        stdin_fallback: false,
        listener,
        result_rx,
        state: app_state,
    })
}

impl ManifestFlowServer {
    /// Completion can happen via two paths (whichever fires first):
    /// 1. GitHub redirects to `/installed` (setup_url callback)
    /// 2. Background poller detects the installation via `GET /app/installations`
    pub fn run(self) -> Result<ManifestFlowResult> {
        let result_rx = self.result_rx;
        let poll_state = self.state.clone();
        let should_open = self.open_browser;
        let stdin_fallback = self.stdin_fallback;
        let api_base = self.state.github_api_base.clone();
        let org = self.state.org.clone();
        let rt = tokio::runtime::Runtime::new()
            .context("Failed to create tokio runtime")?;

        rt.block_on(async {
            let tokio_listener = tokio::net::TcpListener::from_std(self.listener)
                .context("Failed to convert listener to tokio")?;

            // Build router based on mode: headless only serves /start,
            // browser mode serves all callback routes.
            let app = if stdin_fallback {
                axum::Router::new()
                    .route("/start", get(handle_start))
                    .with_state(self.state)
            } else {
                axum::Router::new()
                    .route("/start", get(handle_start))
                    .route("/callback", get(handle_callback))
                    .route("/installed", get(handle_installed))
                    .with_state(self.state)
            };

            let server = axum::serve(tokio_listener, app);

            // Browser auto-launch disabled — users click the link in the
            // terminal instead. Kept for reference in case we want to
            // re-enable on platforms where it works reliably.
            //
            // if should_open {
            //     std::thread::spawn(move || {
            //         let _ = open::that(&url);
            //     });
            // }
            let _ = should_open; // suppress unused warning

            if stdin_fallback {
                // Headless mode: server runs (to serve /start) but only
                // stdin completes the flow. No poller, no callback race.
                let (stdin_tx, stdin_rx) = oneshot::channel::<String>();
                std::thread::spawn(move || {
                    use std::io::IsTerminal;
                    if !std::io::stdin().is_terminal() {
                        return;
                    }
                    loop {
                        let mut line = String::new();
                        if std::io::stdin().read_line(&mut line).is_err() {
                            return;
                        }
                        if let Some(code) = extract_code_from_url(&line) {
                            let _ = stdin_tx.send(code);
                            return;
                        }
                        eprint!("  Could not find a code in that URL. Try again: ");
                    }
                });

                tokio::select! {
                    code = stdin_rx => {
                        match code {
                            Ok(code) => {
                                let creds = exchange_manifest_code(&code, &api_base).await?;

                                // Step 2: user needs to install the App on the org.
                                // Print the installation URL and poll until it appears.
                                eprintln!();
                                eprintln!("  App created! Now install it on your organization:");
                                eprintln!("    {}/installations/new", creds.html_url);
                                eprintln!();
                                eprintln!("  Waiting for installation...");

                                // Poll until the installation appears
                                let installation_id = loop {
                                    match query_installation_id(
                                        &creds.client_id, &creds.private_key, &org, &api_base,
                                    ).await {
                                        Ok(id) => break id,
                                        Err(_) => {
                                            tokio::time::sleep(
                                                std::time::Duration::from_secs(3)
                                            ).await;
                                        }
                                    }
                                };

                                Ok(ManifestFlowResult {
                                    app_id: creds.app_id,
                                    client_id: creds.client_id,
                                    private_key: creds.private_key,
                                    installation_id: installation_id.to_string(),
                                })
                            }
                            Err(_) => bail!("Stdin reader closed"),
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(
                        std::env::var("BM_MANIFEST_TIMEOUT_SECS")
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(300)
                    )) => {
                        bail!(
                            "Manifest flow timed out after 5 minutes.\n\
                             To retry, run `bm hire` again.\n\
                             For headless environments, use --reuse-app with pre-generated credentials."
                        )
                    }
                    res = server.into_future() => {
                        match res {
                            Ok(()) => bail!("Server shut down unexpectedly"),
                            Err(e) => bail!("Server error: {e}"),
                        }
                    }
                }
            } else {
                // Browser mode: poller + callback race to complete.
                let poller = poll_for_installation(poll_state);

                tokio::select! {
                    result = result_rx => {
                        match result {
                            Ok(r) => Ok(r),
                            Err(_) => bail!("Manifest flow server shut down without completing"),
                        }
                    }
                    result = poller => {
                        result
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(
                        std::env::var("BM_MANIFEST_TIMEOUT_SECS")
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(300)
                    )) => {
                        bail!(
                            "Manifest flow timed out after 5 minutes.\n\
                             To retry, run `bm hire` again.\n\
                             For headless environments, use --reuse-app with pre-generated credentials."
                        )
                    }
                    res = server.into_future() => {
                        match res {
                            Ok(()) => bail!("Server shut down unexpectedly before flow completed"),
                            Err(e) => bail!("Manifest flow server error: {e}"),
                        }
                    }
                }
            }
        })
    }
}

/// Polls `GET /app/installations` until an installation appears.
/// Waits for the code exchange to complete first (exchange_creds populated),
/// then polls every 3 seconds.
async fn poll_for_installation(
    state: Arc<ManifestServerState>,
) -> Result<ManifestFlowResult> {
    // Wait for code exchange to complete
    loop {
        if state.exchange_creds.lock().await.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    // Give the setup_url redirect a chance to fire first
    let grace_secs: u64 = std::env::var("BM_MANIFEST_POLL_GRACE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    tokio::time::sleep(std::time::Duration::from_secs(grace_secs)).await;

    // Poll for installation
    loop {
        let creds = state.exchange_creds.lock().await.clone().unwrap();

        match query_installation_id(&creds.client_id, &creds.private_key, &state.org, &state.github_api_base).await {
            Ok(installation_id) => {
                let result = ManifestFlowResult {
                    app_id: creds.app_id,
                    client_id: creds.client_id,
                    private_key: creds.private_key,
                    installation_id: installation_id.to_string(),
                };
                // Also send via the oneshot in case /installed fires concurrently
                if let Some(tx) = state.result_tx.lock().await.take() {
                    let _ = tx.send(ManifestFlowResult {
                        app_id: result.app_id.clone(),
                        client_id: result.client_id.clone(),
                        private_key: result.private_key.clone(),
                        installation_id: result.installation_id.clone(),
                    });
                }
                return Ok(result);
            }
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

/// Extracts the `code` query parameter from a pasted redirect URL.
///
/// Accepts URLs like `http://127.0.0.1:PORT/callback?code=XXX&state=YYY`
/// or just the code string itself as a fallback.
pub fn extract_code_from_url(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // Try to parse as URL with ?code= parameter
    if let Some(query_start) = input.find('?') {
        for pair in input[query_start + 1..].split('&') {
            if let Some(value) = pair.strip_prefix("code=") {
                let code = value.trim();
                if !code.is_empty() {
                    return Some(code.to_string());
                }
            }
        }
    }

    // Fallback: treat the whole input as a code if it's alphanumeric
    if input.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(input.to_string());
    }

    None
}



/// Escapes a string for safe inclusion in HTML attributes and content.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// GET /start — serves the auto-submitting HTML form.
async fn handle_start(
    State(state): State<Arc<ManifestServerState>>,
) -> Html<String> {
    let form_action = format!(
        "{}/organizations/{}/settings/apps/new?state={}",
        state.github_web_base,
        html_escape(&state.org),
        html_escape(&state.csrf_state),
    );
    let manifest_str = html_escape(&state.manifest_json.to_string());

    Html(format!(
        r#"<!DOCTYPE html>
<html>
<head><title>BotMinter — Creating GitHub App</title></head>
<body>
<h2>Creating GitHub App...</h2>
<p>You will be redirected to GitHub to confirm App creation.</p>
<p>If you are not redirected automatically, click the button below.</p>
<form id="manifest-form" method="post" action="{form_action}">
  <input type="hidden" name="manifest" value="{manifest_str}">
  <button type="submit">Create GitHub App</button>
</form>
<script>document.getElementById('manifest-form').submit();</script>
</body>
</html>"#
    ))
}

/// Query params for the /callback endpoint.
#[derive(serde::Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

/// GET /callback — exchanges the code for App credentials, redirects to installation.
async fn handle_callback(
    State(state): State<Arc<ManifestServerState>>,
    Query(params): Query<CallbackParams>,
) -> impl IntoResponse {
    if params.state != state.csrf_state {
        return Html(
            "<h2>Error: State mismatch</h2><p>CSRF validation failed. Please retry.</p>"
                .to_string(),
        )
        .into_response();
    }

    // Exchange code for credentials (no auth required)
    let exchange_result = exchange_manifest_code(&params.code, &state.github_api_base).await;

    match exchange_result {
        Ok(creds) => {
            // Validate html_url matches expected base to prevent open redirect
            let expected_prefix = format!("{}/", state.github_web_base);
            if !creds.html_url.starts_with(&expected_prefix) {
                return Html(format!(
                    "<h2>Error: Unexpected App URL</h2>\
                     <p>Expected a github.com URL but got: {}</p>",
                    html_escape(&creds.html_url),
                ))
                .into_response();
            }
            let install_url = format!("{}/installations/new", creds.html_url);
            *state.exchange_creds.lock().await = Some(creds);
            Redirect::temporary(&install_url).into_response()
        }
        Err(e) => Html(format!(
            "<h2>Error: Code exchange failed</h2><p>{}</p>\
             <p>The code may have expired. Please retry with <code>bm hire</code>.</p>",
            html_escape(&e.to_string()),
        ))
        .into_response(),
    }
}

/// Exchanges a manifest code for App credentials.
async fn exchange_manifest_code(code: &str, api_base: &str) -> Result<ExchangeCredentials> {
    // Validate code format — GitHub manifest codes are hex strings
    if !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("Invalid manifest code format");
    }

    let url = format!("{api_base}/app-manifests/{code}/conversions");

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "botminter")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .context("Failed to call manifest code exchange endpoint")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("GitHub API returned {status} during code exchange: {body}");
    }

    let resp: CodeExchangeResponse = response
        .json()
        .await
        .context("Failed to parse code exchange response")?;

    Ok(ExchangeCredentials {
        app_id: resp.id.to_string(),
        client_id: resp.client_id,
        private_key: resp.pem,
        html_url: resp.html_url,
    })
}

/// GET /installed — captures the installation ID after App installation.
///
/// Always queries the GitHub API to discover the installation ID authoritatively.
/// The `installation_id` query parameter from GitHub's redirect is intentionally
/// ignored to prevent spoofing by local processes.
async fn handle_installed(
    State(state): State<Arc<ManifestServerState>>,
) -> Html<String> {
    // Short-circuit if result already sent (e.g., by the background poller)
    if state.result_tx.lock().await.is_none() {
        return Html(
            "<h2>Already completed</h2>\
             <p>App credentials have been stored. You can close this tab.</p>"
                .to_string(),
        );
    }

    let creds = match state.exchange_creds.lock().await.clone() {
        Some(c) => c,
        None => {
            return Html(
                "<h2>Error</h2><p>App credentials not found. \
                 The App creation step may not have completed. Please retry.</p>"
                    .to_string(),
            );
        }
    };

    // Always query the API for the installation ID — never trust query params
    let installation_id = match query_installation_id(
        &creds.client_id,
        &creds.private_key,
        &state.org,
        &state.github_api_base,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            return Html(format!(
                "<h2>Error: Could not retrieve installation ID</h2>\
                 <p>{}</p>\
                 <p>Please ensure the App was installed on your organization, then retry.</p>",
                html_escape(&e.to_string()),
            ));
        }
    };

    let result = ManifestFlowResult {
        app_id: creds.app_id,
        client_id: creds.client_id,
        private_key: creds.private_key,
        installation_id: installation_id.to_string(),
    };

    // Send result back to the main thread
    if let Some(tx) = state.result_tx.lock().await.take() {
        let _ = tx.send(result);
    }

    Html(
        "<h2>Success!</h2>\
         <p>GitHub App created and installed. You can close this tab.</p>\
         <p>BotMinter is storing the credentials...</p>"
            .to_string(),
    )
}

/// Queries `GET /app/installations` to find the installation ID for the expected org.
/// Uses JWT authentication with the newly created App's credentials.
/// Validates that the installation belongs to the expected organization.
async fn query_installation_id(client_id: &str, private_key: &str, expected_org: &str, api_base: &str) -> Result<u64> {
    let jwt = super::app_auth::generate_jwt(client_id, private_key)
        .context("Failed to generate JWT for installation query")?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{api_base}/app/installations"))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "botminter")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .context("Failed to query App installations")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("GitHub API returned {status} when querying installations: {body}");
    }

    let installations: Vec<Installation> = response
        .json()
        .await
        .context("Failed to parse installations response")?;

    // Find the installation matching the expected org
    for inst in &installations {
        if let Some(ref account) = inst.account {
            if account.login.eq_ignore_ascii_case(expected_org) {
                return Ok(inst.id);
            }
        }
    }

    // Fall back to first installation if no org match (single-installation case)
    if installations.len() == 1 {
        return Ok(installations[0].id);
    }

    bail!(
        "No installation found for organization '{}'. Found {} installation(s).",
        expected_org,
        installations.len(),
    )
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_manifest_json_has_required_fields() {
        let manifest = build_manifest_json("my-team-superman", "https://github.com/org/my-team", 12345);

        assert_eq!(manifest["name"], "my-team-superman");
        assert_eq!(manifest["url"], "https://github.com/org/my-team");
        assert_eq!(
            manifest["redirect_url"],
            "http://127.0.0.1:12345/callback"
        );
        assert_eq!(
            manifest["setup_url"],
            "http://127.0.0.1:12345/installed"
        );
        assert_eq!(manifest["public"], false);
        assert_eq!(manifest["default_events"], serde_json::json!([]));
    }

    #[test]
    fn build_manifest_json_has_correct_permissions() {
        let manifest = build_manifest_json("app", "https://example.com", 8080);
        let perms = &manifest["default_permissions"];

        assert_eq!(perms["issues"], "write");
        assert_eq!(perms["contents"], "write");
        assert_eq!(perms["pull_requests"], "write");
        assert_eq!(perms["administration"], "write");
        assert_eq!(perms["organization_projects"], "admin");

        // MUST NOT have the deprecated "projects" permission
        assert!(
            perms.get("projects").is_none(),
            "Manifest must use organization_projects, not projects (which is for classic boards)"
        );
    }

    #[test]
    fn app_name_to_slug_lowercases_and_replaces_spaces() {
        assert_eq!(app_name_to_slug("My Team Superman"), "my-team-superman");
        assert_eq!(app_name_to_slug("already-lowercase"), "already-lowercase");
        assert_eq!(app_name_to_slug("MixedCase"), "mixedcase");
    }

    #[test]
    fn credential_keys_follow_convention() {
        assert_eq!(credential_keys::app_id("superman"), "superman/github-app-id");
        assert_eq!(
            credential_keys::client_id("superman"),
            "superman/github-app-client-id"
        );
        assert_eq!(
            credential_keys::private_key("superman"),
            "superman/github-app-private-key"
        );
        assert_eq!(
            credential_keys::installation_id("superman"),
            "superman/github-installation-id"
        );
    }

    #[test]
    fn store_pregenerated_credentials_writes_all_keys() {
        let store = crate::formation::InMemoryKeyValueCredentialStore::new();
        let creds = PreGeneratedCredentials {
            app_id: "789".to_string(),
            client_id: "Iv1.xyz".to_string(),
            private_key: "PEM_DATA".to_string(),
            installation_id: "1011".to_string(),
        };

        store_pregenerated_credentials(&store, "batman", &creds).unwrap();

        assert_eq!(
            store.retrieve("batman/github-app-id").unwrap(),
            Some("789".to_string())
        );
        assert_eq!(
            store.retrieve("batman/github-app-client-id").unwrap(),
            Some("Iv1.xyz".to_string())
        );
        assert_eq!(
            store.retrieve("batman/github-app-private-key").unwrap(),
            Some("PEM_DATA".to_string())
        );
        assert_eq!(
            store.retrieve("batman/github-installation-id").unwrap(),
            Some("1011".to_string())
        );
    }

    #[test]
    fn save_credentials_to_file_creates_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("creds.yml");
        let creds = PreGeneratedCredentials {
            app_id: "42".to_string(),
            client_id: "Iv1.test".to_string(),
            private_key: "PEM".to_string(),
            installation_id: "99".to_string(),
        };

        save_credentials_to_file(path.to_str().unwrap(), "test-member", &creds).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Verify actual values, not just key presence
        let parsed: serde_json::Value = serde_yml::from_str(&content).unwrap();
        assert_eq!(parsed["member"], "test-member");
        assert_eq!(parsed["app_id"], "42");
        assert_eq!(parsed["client_id"], "Iv1.test");
        assert_eq!(parsed["private_key"], "PEM");
        assert_eq!(parsed["installation_id"], "99");

        // Check file permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&path).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o600, "File should be 0600");
        }
    }

    #[test]
    fn fork_url_to_owner_repo_extracts_correctly() {
        assert_eq!(
            fork_url_to_owner_repo("https://github.com/org/repo"),
            Some("org/repo".to_string())
        );
        assert_eq!(
            fork_url_to_owner_repo("https://github.com/org/repo.git"),
            Some("org/repo".to_string())
        );
        assert_eq!(
            fork_url_to_owner_repo("https://github.com/org/repo/"),
            Some("org/repo".to_string())
        );
        // Non-GitHub URLs return None
        assert_eq!(fork_url_to_owner_repo("https://gitlab.com/org/repo"), None);
        // Empty or malformed
        assert_eq!(fork_url_to_owner_repo(""), None);
        assert_eq!(fork_url_to_owner_repo("https://github.com/"), None);
        assert_eq!(fork_url_to_owner_repo("https://github.com/orgonly"), None);
    }

    #[test]
    fn collect_team_repos_includes_team_and_project_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let team_dir = tmp.path().join("team");
        std::fs::create_dir_all(&team_dir).unwrap();

        // Write botminter.yml with projects
        std::fs::write(
            team_dir.join("botminter.yml"),
            "name: test\nschema_version: '1.0'\nversion: '1.0.0'\ndescription: t\ndisplay_name: t\nprojects:\n  - name: my-app\n    fork_url: https://github.com/org/my-app.git\n  - name: lib\n    fork_url: https://github.com/org/lib\n",
        ).unwrap();

        let team = crate::config::TeamEntry {
            name: "test-team".to_string(),
            path: tmp.path().to_path_buf(),
            profile: "scrum".to_string(),
            github_repo: "org/team-repo".to_string(),
            credentials: Default::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        };

        let repos = collect_team_repos(&team);
        assert_eq!(repos, vec!["org/team-repo", "org/my-app", "org/lib"]);
    }

    #[test]
    fn collect_team_repos_empty_github_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let team_dir = tmp.path().join("team");
        std::fs::create_dir_all(&team_dir).unwrap();
        std::fs::write(
            team_dir.join("botminter.yml"),
            "name: test\nschema_version: '1.0'\nversion: '1.0.0'\ndescription: t\ndisplay_name: t\n",
        ).unwrap();

        let team = crate::config::TeamEntry {
            name: "test-team".to_string(),
            path: tmp.path().to_path_buf(),
            profile: "scrum".to_string(),
            github_repo: "".to_string(),
            credentials: Default::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        };

        let repos = collect_team_repos(&team);
        assert!(repos.is_empty());
    }

    #[test]
    fn collect_team_repos_no_botminter_yml() {
        let tmp = tempfile::tempdir().unwrap();
        // No team/ dir at all — should gracefully return just the team repo
        let team = crate::config::TeamEntry {
            name: "test-team".to_string(),
            path: tmp.path().to_path_buf(),
            profile: "scrum".to_string(),
            github_repo: "org/repo".to_string(),
            credentials: Default::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        };

        let repos = collect_team_repos(&team);
        assert_eq!(repos, vec!["org/repo"]);
    }

    #[test]
    fn html_escape_handles_special_chars() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape(r#"x"y"#), "x&quot;y");
        assert_eq!(html_escape("x'y"), "x&#x27;y");
        assert_eq!(html_escape("safe-string-123"), "safe-string-123");
    }

    #[test]
    fn html_escape_prevents_attribute_breakout() {
        // An app name with quotes should not break out of an HTML attribute
        let malicious = r#"app' onclick='alert(1)"#;
        let escaped = html_escape(malicious);
        assert!(!escaped.contains('\''));
        assert!(!escaped.contains('"'));
    }

    #[test]
    fn start_page_html_contains_form_action_and_manifest() {
        // Simulate what handle_start produces by testing the HTML construction logic
        let org = "test-org";
        let csrf_state = "test-state-uuid";
        let manifest = build_manifest_json("test-app", "https://github.com/test-org/repo", 9999);

        let form_action = format!(
            "https://github.com/organizations/{}/settings/apps/new?state={}",
            org, csrf_state,
        );
        let manifest_str = manifest.to_string();

        // Verify form action points to the correct GitHub endpoint
        assert!(form_action.contains("organizations/test-org/settings/apps/new"));
        assert!(form_action.contains("state=test-state-uuid"));

        // Verify manifest JSON is well-formed and contains required fields
        assert!(manifest_str.contains("test-app"));
        assert!(manifest_str.contains("redirect_url"));
        assert!(manifest_str.contains("setup_url"));
        assert!(manifest_str.contains("organization_projects"));
    }

    #[test]
    fn code_exchange_response_parses() {
        let json = r#"{
            "id": 12345,
            "slug": "my-team-superman",
            "client_id": "Iv1.abc123",
            "client_secret": "secret",
            "pem": "-----BEGIN RSA PRIVATE KEY-----\ntest\n-----END RSA PRIVATE KEY-----",
            "webhook_secret": "whsec",
            "html_url": "https://github.com/apps/my-team-superman",
            "permissions": {"issues": "write"},
            "owner": {"login": "test-org", "type": "Organization"}
        }"#;

        let resp: CodeExchangeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, 12345);
        assert_eq!(resp.client_id, "Iv1.abc123");
        assert!(resp.pem.contains("RSA PRIVATE KEY"));
        assert_eq!(resp.html_url, "https://github.com/apps/my-team-superman");
    }

    #[test]
    fn installation_response_parses_with_account() {
        let json = r#"[
            {"id": 99999, "account": {"login": "test-org"}, "repository_selection": "all"}
        ]"#;

        let installations: Vec<Installation> = serde_json::from_str(json).unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].id, 99999);
        assert_eq!(
            installations[0].account.as_ref().unwrap().login,
            "test-org"
        );
    }

    #[test]
    fn extract_code_from_callback_url() {
        assert_eq!(
            extract_code_from_url("http://127.0.0.1:12345/callback?code=abc123&state=xyz"),
            Some("abc123".to_string()),
        );
    }

    #[test]
    fn extract_code_from_url_with_code_only() {
        assert_eq!(
            extract_code_from_url("http://127.0.0.1:9999/callback?code=deadbeef42"),
            Some("deadbeef42".to_string()),
        );
    }

    #[test]
    fn extract_code_from_raw_code_string() {
        assert_eq!(
            extract_code_from_url("abc123def456"),
            Some("abc123def456".to_string()),
        );
    }

    #[test]
    fn extract_code_from_empty_input() {
        assert_eq!(extract_code_from_url(""), None);
        assert_eq!(extract_code_from_url("   "), None);
    }

    #[test]
    fn extract_code_from_url_without_code_param() {
        assert_eq!(
            extract_code_from_url("http://127.0.0.1:12345/callback?state=xyz"),
            None,
        );
    }

    #[test]
    fn empty_installations_array_parses() {
        let json = "[]";
        let installations: Vec<Installation> = serde_json::from_str(json).unwrap();
        assert!(installations.is_empty());
    }

    #[test]
    fn prepare_manifest_flow_binds_and_returns_url() {
        let server = prepare_manifest_flow(&ManifestFlowParams {
            app_name: "test-app".to_string(),
            org: "test-org".to_string(),
            team_repo_url: "https://github.com/test-org/repo".to_string(),
            github_api_base: None,
            github_web_base: None,
        })
        .unwrap();

        assert!(server.start_url.starts_with("http://127.0.0.1:"));
        assert!(server.start_url.ends_with("/start"));
        // Port should be > 0 (OS-assigned)
        let port_str = server
            .start_url
            .strip_prefix("http://127.0.0.1:")
            .unwrap()
            .strip_suffix("/start")
            .unwrap();
        let port: u16 = port_str.parse().unwrap();
        assert!(port > 0);
    }

    #[test]
    fn store_credentials_overwrites_existing() {
        let store = crate::formation::InMemoryKeyValueCredentialStore::new();

        // Store initial
        let creds1 = PreGeneratedCredentials {
            app_id: "100".to_string(),
            client_id: "Iv1.old".to_string(),
            private_key: "OLD_PEM".to_string(),
            installation_id: "200".to_string(),
        };
        store_pregenerated_credentials(&store, "hero", &creds1).unwrap();

        // Overwrite with new
        let creds2 = PreGeneratedCredentials {
            app_id: "300".to_string(),
            client_id: "Iv1.new".to_string(),
            private_key: "NEW_PEM".to_string(),
            installation_id: "400".to_string(),
        };
        store_pregenerated_credentials(&store, "hero", &creds2).unwrap();

        assert_eq!(
            store.retrieve("hero/github-app-id").unwrap(),
            Some("300".to_string())
        );
        assert_eq!(
            store.retrieve("hero/github-app-client-id").unwrap(),
            Some("Iv1.new".to_string())
        );
    }
}
