use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};

use super::api;
use super::config::{load_poll_state, save_poll_state, DaemonConfig, DaemonPaths};
use super::event::{
    is_relevant_event, load_webhook_secret, poll_github_events, resolve_github_repo,
    validate_webhook_signature,
};
use super::log::daemon_log;
use super::process::handle_member_launch;
use crate::config as app_config;
use crate::formation::AppCredentialsCached;
use crate::web::state::WebState;
use crate::web::web_router;

/// Shared state for axum handlers.
#[derive(Clone)]
pub(super) struct DaemonState {
    pub(super) team_name: String,
    pub(super) paths: Arc<DaemonPaths>,
    pub(super) webhook_secret: Option<String>,
    pub(super) shutdown: Arc<AtomicBool>,
    pub(super) mode: String,
    pub(super) started_at: Option<std::time::Instant>,
    /// Cached config loaded once at daemon startup. API handlers use this
    /// instead of re-reading from disk on every request, which avoids failures
    /// when the HOME directory changes (e.g., in E2E tests).
    pub(super) config: Arc<app_config::BotminterConfig>,
    pub(super) team_entry: Arc<app_config::TeamEntry>,
    /// In-memory cache of App credentials for members that have been started.
    /// Used by the background refresh loop to re-sign JWTs without re-reading keyring.
    pub(super) app_credentials: Arc<Mutex<HashMap<String, AppCredentialsCached>>>,
}

/// Runs the daemon event loop. Called by the hidden `bm daemon-run` command.
/// This function does not return until the daemon is signaled to stop.
pub fn run_daemon(
    team_name: &str,
    mode: &str,
    port: u16,
    interval: u64,
    bind: &str,
) -> Result<()> {
    // Resolve the isolated keyring D-Bus address BEFORE creating the tokio
    // runtime. `with_keyring_dbus` in credential.rs swaps DBUS_SESSION_BUS_ADDRESS
    // via `std::env::set_var`, which is unsound in multi-threaded processes.
    // By setting DBUS_SESSION_BUS_ADDRESS here and removing BM_KEYRING_DBUS,
    // `with_keyring_dbus` becomes a no-op and the keyring uses the right
    // D-Bus session without any env var mutation during runtime.
    if let Ok(dbus) = std::env::var("BM_KEYRING_DBUS") {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &dbus);
        std::env::remove_var("BM_KEYRING_DBUS");
    }

    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
    rt.block_on(run_daemon_async(team_name, mode, port, interval, bind))
}

async fn run_daemon_async(
    team_name: &str,
    mode: &str,
    port: u16,
    interval: u64,
    bind: &str,
) -> Result<()> {
    // NOTE: Do NOT set SIGCHLD=SIG_IGN here. While it prevents zombie children,
    // it also breaks Command::output() (used by gh api in poll mode) because
    // the auto-reaped child causes waitpid to return ECHILD. Instead, each
    // spawn site calls reap_child() which spawns a thread that waits on the
    // Child handle, calling waitpid() to reap the process on exit.

    let paths = Arc::new(DaemonPaths::new(team_name)?);
    let shutdown = Arc::new(AtomicBool::new(false));

    daemon_log(&paths, "INFO", &format!("Daemon starting in {} mode", mode));

    // Load config once at startup and cache it. API handlers use these
    // cached values instead of re-reading config from disk on every request.
    let cfg = app_config::load()
        .context("Daemon failed to load config at startup")?;
    let team_entry = app_config::resolve_team(&cfg, Some(team_name))
        .context("Daemon failed to resolve team at startup")?
        .clone();

    let state = DaemonState {
        team_name: team_name.to_string(),
        paths: Arc::clone(&paths),
        webhook_secret: load_webhook_secret(team_name),
        shutdown: Arc::clone(&shutdown),
        mode: mode.to_string(),
        started_at: Some(std::time::Instant::now()),
        config: Arc::new(cfg),
        team_entry: Arc::new(team_entry),
        app_credentials: Arc::new(Mutex::new(HashMap::new())),
    };

    // Resolve config path for the web API (console routes)
    let config_path = app_config::config_path()
        .unwrap_or_else(|_| std::path::PathBuf::from("~/.botminter/config.yml"));
    let web_state = WebState {
        config_path: Arc::new(config_path),
    };

    // CORS: allow requests from localhost dev servers (Vite on :5173, etc.)
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            origin
                .to_str()
                .map(|o| {
                    o.starts_with("http://localhost:") || o.starts_with("http://127.0.0.1:")
                })
                .unwrap_or(false)
        }))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/webhook", post(webhook_handler))
        .route("/health", get(health_handler))
        // Member lifecycle API
        .route("/api/members/start", post(api::start_members_handler))
        .route("/api/members/stop", post(api::stop_members_handler))
        .route("/api/members", get(api::list_members_handler))
        .route("/api/health", get(api::health_check_handler))
        // Loop management API
        .route("/api/loops/start", post(api::start_loop_handler))
        .with_state(state.clone())
        .merge(web_router(web_state))
        .layer(cors);

    // In poll mode, spawn the background poll loop
    if mode == "poll" {
        let poll_team = team_name.to_string();
        let poll_paths = Arc::clone(&paths);
        let poll_shutdown = Arc::clone(&shutdown);
        tokio::spawn(async move {
            run_poll_loop(&poll_team, &poll_paths, interval, &poll_shutdown).await;
        });
    }

    let addr: SocketAddr = format!("{}:{}", bind, port)
        .parse()
        .with_context(|| format!("Invalid bind address: {}:{}", bind, port))?;

    daemon_log(
        &paths,
        "INFO",
        &format!(
            "{} server listening on {}",
            match mode {
                "webhook" => "Webhook",
                "poll" => "Poll",
                _ => mode,
            },
            addr
        ),
    );
    daemon_log(
        &paths,
        "INFO",
        &format!("Console available at http://{}:{}", bind, port),
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    // After binding, write the daemon config with the actual port. This is
    // critical when port=0 (OS-assigned): the parent process and clients
    // read this file to discover the daemon's address.
    let actual_addr = listener.local_addr()
        .context("Failed to get listener local address")?;
    let daemon_cfg = DaemonConfig {
        team: team_name.to_string(),
        mode: mode.to_string(),
        port: actual_addr.port(),
        interval_secs: interval,
        pid: std::process::id(),
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let cfg_contents = serde_json::to_string_pretty(&daemon_cfg)
        .context("Failed to serialize daemon config")?;
    std::fs::write(paths.config(), &cfg_contents)
        .with_context(|| format!("Failed to write daemon config to {}", paths.config().display()))?;

    daemon_log(
        &paths,
        "INFO",
        &format!("Daemon config written (port={})", actual_addr.port()),
    );

    let shutdown_flag = Arc::clone(&shutdown);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_flag))
        .await
        .context("Server error")?;

    // Clean up: stop all running members before exiting.
    // Members are fire-and-forget (PIDs in state.json), so the daemon must
    // actively terminate them on shutdown. Use force=true to stay within the
    // 30s budget that stop_daemon() allows before SIGKILL'ing us.
    //
    // NOTE: This calls stop_local_members directly (not through the API handler),
    // so it does NOT write suspended markers. This is intentional — the daemon
    // itself is going away, so suspension is meaningless.
    daemon_log(&paths, "INFO", "Stopping members before exit...");
    let cleanup_team = team_name.to_string();
    let cleanup_cfg = app_config::load().ok();
    if let Some(cfg) = cleanup_cfg {
        if let Ok(team) = app_config::resolve_team(&cfg, Some(&cleanup_team)) {
            if let Err(e) = crate::formation::stop_local_members(team, &cfg, None, true) {
                daemon_log(&paths, "WARN", &format!("Member cleanup error: {e}"));
            }
        }
    }

    daemon_log(&paths, "INFO", "Daemon stopped");
    Ok(())
}

/// Waits for SIGTERM or SIGINT, then sets the shutdown flag.
async fn shutdown_signal(shutdown: Arc<AtomicBool>) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    shutdown.store(true, Ordering::SeqCst);
}

/// Axum handler for POST /webhook.
async fn webhook_handler(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s.to_string(),
        Err(_) => {
            daemon_log(&state.paths, "ERROR", "Failed to read request body as UTF-8");
            return StatusCode::BAD_REQUEST;
        }
    };

    // Validate signature if webhook secret is configured
    if let Some(ref secret) = state.webhook_secret {
        let sig_header = headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if !validate_webhook_signature(secret, &body_str, sig_header.as_deref()) {
            daemon_log(&state.paths, "WARN", "Webhook signature validation failed");
            return StatusCode::FORBIDDEN;
        }
    }

    // Parse event type from header
    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(event_type) = event_type {
        if is_relevant_event(&event_type) {
            daemon_log(
                &state.paths,
                "INFO",
                &format!("Received relevant event: {}", event_type),
            );
            let team = state.team_name.clone();
            let paths = Arc::clone(&state.paths);
            let shutdown = Arc::clone(&state.shutdown);
            tokio::task::spawn_blocking(move || {
                handle_member_launch(&team, &paths, &shutdown);
            });
        } else {
            daemon_log(
                &state.paths,
                "DEBUG",
                &format!("Ignoring irrelevant event: {}", event_type),
            );
        }
    }

    StatusCode::OK
}

/// Axum handler for GET /health.
async fn health_handler() -> impl IntoResponse {
    let version = env!("CARGO_PKG_VERSION");
    let body = serde_json::json!({ "ok": true, "version": version });
    (StatusCode::OK, axum::Json(body))
}

/// Runs the poll loop as a background async task.
async fn run_poll_loop(
    team_name: &str,
    paths: &DaemonPaths,
    interval: u64,
    shutdown: &Arc<AtomicBool>,
) {
    daemon_log(
        paths,
        "INFO",
        &format!("Poll mode started, interval: {}s", interval),
    );

    let poll_state_file = paths.poll_state();
    let mut poll_state = load_poll_state(&poll_state_file);

    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(interval));
    // First tick fires immediately — skip it to match original behavior of polling after
    // a sleep. Actually, the original code polled first then slept, so let the first tick
    // proceed.

    loop {
        ticker.tick().await;

        if shutdown.load(Ordering::SeqCst) {
            daemon_log(
                paths,
                "INFO",
                "Received shutdown signal, stopping poll loop",
            );
            break;
        }

        // All poll operations (resolve_github_repo, poll_github_events,
        // handle_member_launch) are blocking sync calls that spawn subprocesses
        // or do file I/O. Run them on the blocking thread pool to avoid starving
        // the async runtime's worker threads.
        let poll_team = team_name.to_string();
        let poll_state_clone = poll_state.clone();
        let poll_paths = paths.clone();
        let poll_shutdown = Arc::clone(shutdown);

        let result = tokio::task::spawn_blocking(move || {
            let github_repo = resolve_github_repo(&poll_team)?;
            let events = poll_github_events(&github_repo, &poll_state_clone)?;
            let relevant_count = events
                .iter()
                .filter(|e| is_relevant_event(&e.event_type))
                .count();

            if relevant_count > 0 {
                daemon_log(
                    &poll_paths,
                    "INFO",
                    &format!("Found {} relevant event(s)", relevant_count),
                );
                handle_member_launch(&poll_team, &poll_paths, &poll_shutdown);
            }

            Ok::<_, anyhow::Error>(events)
        })
        .await;

        match result {
            Ok(Ok(events)) => {
                if let Some(latest) = events.first() {
                    poll_state.last_event_id = Some(latest.id.clone());
                }
                poll_state.last_poll_at = Some(chrono::Utc::now().to_rfc3339());
                save_poll_state(&poll_state_file, &poll_state);
            }
            Ok(Err(e)) => {
                daemon_log(
                    paths,
                    "ERROR",
                    &format!("Poll cycle failed: {:#}", e),
                );
            }
            Err(e) => {
                daemon_log(
                    paths,
                    "ERROR",
                    &format!("Poll task panicked: {}", e),
                );
            }
        }
    }

    daemon_log(paths, "INFO", "Poll loop stopped");
}
