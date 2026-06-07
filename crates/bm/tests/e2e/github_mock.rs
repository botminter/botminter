//! Mock GitHub server for manifest flow E2E testing.
//!
//! Handles the GitHub-specific endpoints used by the manifest flow:
//! - `POST /organizations/{org}/settings/apps/new` — form submission, auto-approves
//! - `POST /app-manifests/{code}/conversions` — code exchange
//! - `GET /app/installations` — returns mock installation
//! - `GET /users/{owner}` — validates org type
//!
//! Built on top of `oauth2-test-server` for server infrastructure.

use std::net::TcpListener;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Json;
use tokio::task::JoinHandle;

/// A mock GitHub server for testing the manifest flow.
pub struct GitHubMock {
    #[allow(dead_code)]
    pub port: u16,
    pub base_url: String,
    _handle: JoinHandle<()>,
}

struct MockState {
    base_url: String,
    /// The oauth2-test-server's AppState — provides the RSA key pair
    oauth_state: oauth2_test_server::AppState,
}

#[derive(serde::Deserialize)]
struct AppNewParams {
    state: String,
}

#[derive(serde::Deserialize)]
struct ManifestForm {
    manifest: String,
}

impl GitHubMock {
    /// Starts the mock GitHub server on a random port.
    /// Must be called from within a tokio runtime.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}");

        // Use oauth2-test-server to generate RSA keys and provide OAuth infrastructure
        let oauth_config = oauth2_test_server::IssuerConfig {
            port: 0,
            ..Default::default()
        };
        let oauth_state = oauth2_test_server::store::AppState::new(oauth_config);
        let oauth_router = oauth2_test_server::router::build_router(oauth_state.clone());

        let mock_state = Arc::new(MockState {
            base_url: base_url.clone(),
            oauth_state,
        });

        let github_router = axum::Router::new()
            .route(
                "/organizations/{org}/settings/apps/new",
                post(handle_app_new),
            )
            .route(
                "/app-manifests/{code}/conversions",
                post(handle_code_exchange),
            )
            .route("/app/installations", get(handle_installations))
            .route("/users/{owner}", get(handle_user_type))
            .with_state(mock_state);

        let app = github_router.merge(oauth_router);

        let handle = tokio::spawn(async move {
            let tl = tokio::net::TcpListener::from_std(listener).unwrap();
            axum::serve(tl, app).await.unwrap();
        });

        eprintln!("[github-mock] listening on {base_url}");

        Self {
            port,
            base_url,
            _handle: handle,
        }
    }
}

/// POST /organizations/{org}/settings/apps/new?state={state}
///
/// Simulates GitHub's App creation page. Reads the manifest from the form body,
/// extracts the `redirect_url`, and redirects to it with a mock code + the state.
async fn handle_app_new(
    State(_state): State<Arc<MockState>>,
    Path(org): Path<String>,
    Query(params): Query<AppNewParams>,
    axum::Form(form): axum::Form<ManifestForm>,
) -> impl axum::response::IntoResponse {
    eprintln!("[github-mock] POST /organizations/{org}/settings/apps/new");

    // Parse the manifest to extract redirect_url
    let manifest: serde_json::Value = match serde_json::from_str(&form.manifest) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[github-mock] failed to parse manifest: {e}");
            return Html(format!("bad manifest: {e}")).into_response();
        }
    };

    let redirect_url = match manifest["redirect_url"].as_str() {
        Some(url) => url.to_string(),
        None => {
            return Html("manifest missing redirect_url".to_string()).into_response();
        }
    };

    // Redirect to the callback with a mock code
    let redirect = format!("{redirect_url}?code=a1b2c3d4e5f6789012345678&state={}", params.state);
    eprintln!("[github-mock] redirecting to {redirect}");
    Redirect::temporary(&redirect).into_response()
}

/// POST /app-manifests/{code}/conversions
///
/// Returns mock App credentials. The code is accepted without validation.
async fn handle_code_exchange(
    State(state): State<Arc<MockState>>,
    Path(code): Path<String>,
) -> Json<serde_json::Value> {
    eprintln!("[github-mock] POST /app-manifests/{code}/conversions");
    Json(serde_json::json!({
        "id": 123456,
        "slug": "mock-test-app",
        "client_id": "Iv1.mock_client_id_e2e",
        "client_secret": "mock_client_secret",
        "pem": &state.oauth_state.keys.private_pem,
        "webhook_secret": "mock_webhook_secret",
        "html_url": format!("{}/apps/mock-test-app", state.base_url),
        "permissions": {"issues": "write", "contents": "write"},
        "owner": {"login": "test-org", "type": "Organization"}
    }))
}

/// GET /app/installations
///
/// Returns a single mock installation for the test org.
async fn handle_installations(State(_state): State<Arc<MockState>>) -> Json<serde_json::Value> {
    eprintln!("[github-mock] GET /app/installations");
    Json(serde_json::json!([
        {
            "id": 99999,
            "account": {"login": "test-org", "type": "Organization"},
            "repository_selection": "all",
            "permissions": {"issues": "write", "contents": "write"}
        }
    ]))
}

/// GET /users/{owner}
///
/// Returns Organization type for any owner (needed by validate_is_org).
async fn handle_user_type(State(_state): State<Arc<MockState>>, Path(owner): Path<String>) -> Json<serde_json::Value> {
    eprintln!("[github-mock] GET /users/{owner}");
    Json(serde_json::json!({
        "login": owner,
        "type": "Organization"
    }))
}
