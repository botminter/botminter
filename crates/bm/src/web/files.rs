use std::fs;
use std::path::{Path, PathBuf};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::state::WebState;
use crate::config;

/// GET /api/teams/:team/files/*path — read a file from the team repo.
pub async fn read_file(
    State(state): State<WebState>,
    AxumPath((team_name, file_path)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    match do_read_file(&state, &team_name, &file_path) {
        Ok(resp) => (StatusCode::OK, Json(serde_json::json!(resp))).into_response(),
        Err((status, msg)) => {
            (status, Json(serde_json::json!({ "error": msg }))).into_response()
        }
    }
}

/// PUT /api/teams/:team/files/*path — write a file and git commit.
pub async fn write_file(
    State(state): State<WebState>,
    AxumPath((team_name, file_path)): AxumPath<(String, String)>,
    Json(body): Json<FileWriteRequest>,
) -> impl IntoResponse {
    match do_write_file(&state, &team_name, &file_path, &body.content).await {
        Ok(resp) => (StatusCode::OK, Json(serde_json::json!(resp))).into_response(),
        Err((status, msg)) => {
            (status, Json(serde_json::json!({ "error": msg }))).into_response()
        }
    }
}

/// GET /api/teams/:team/tree?path=... — list directory entries.
pub async fn list_tree(
    State(state): State<WebState>,
    AxumPath(team_name): AxumPath<String>,
    Query(params): Query<TreeQuery>,
) -> impl IntoResponse {
    let rel_path = params.path.as_deref().unwrap_or("");
    match do_list_tree(&state, &team_name, rel_path) {
        Ok(resp) => (StatusCode::OK, Json(serde_json::json!(resp))).into_response(),
        Err((status, msg)) => {
            (status, Json(serde_json::json!({ "error": msg }))).into_response()
        }
    }
}

// ── Path security ──────────────────────────────────────────

/// Validates and resolves a relative path within the team repo root.
/// Returns the canonical absolute path on success, or a 403/404 error.
fn safe_resolve(team_path: &Path, relative: &str) -> Result<PathBuf, (StatusCode, String)> {
    // Reject empty paths
    if relative.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "File path is required".to_string(),
        ));
    }

    // Reject absolute paths
    if relative.starts_with('/') || relative.starts_with('\\') {
        return Err((
            StatusCode::FORBIDDEN,
            "Absolute paths are not allowed".to_string(),
        ));
    }

    // URL-decode the path to catch %2e%2e attacks
    let decoded = percent_decode(relative);

    // Reject paths containing .. segments (before any filesystem access)
    for segment in decoded.split('/') {
        if segment == ".." {
            return Err((
                StatusCode::FORBIDDEN,
                "Path traversal is not allowed".to_string(),
            ));
        }
    }
    // Also check backslash-separated segments (Windows-style)
    for segment in decoded.split('\\') {
        if segment == ".." {
            return Err((
                StatusCode::FORBIDDEN,
                "Path traversal is not allowed".to_string(),
            ));
        }
    }

    // Canonicalize the repo root
    let canonical_root = team_path.canonicalize().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to resolve team path".to_string(),
        )
    })?;

    // Build the target path and check it exists
    let target = team_path.join(&decoded);
    if !target.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Path not found: {}", relative),
        ));
    }

    // Canonicalize the resolved path (follows symlinks)
    let canonical_target = target.canonicalize().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to resolve file path".to_string(),
        )
    })?;

    // Verify the canonical target is within the canonical root
    if !canonical_target.starts_with(&canonical_root) {
        return Err((
            StatusCode::FORBIDDEN,
            "Path traversal is not allowed".to_string(),
        ));
    }

    Ok(canonical_target)
}

/// Like safe_resolve but allows the target to not exist yet (for writes).
fn safe_resolve_for_write(
    team_path: &Path,
    relative: &str,
) -> Result<PathBuf, (StatusCode, String)> {
    if relative.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "File path is required".to_string(),
        ));
    }

    if relative.starts_with('/') || relative.starts_with('\\') {
        return Err((
            StatusCode::FORBIDDEN,
            "Absolute paths are not allowed".to_string(),
        ));
    }

    let decoded = percent_decode(relative);

    for segment in decoded.split('/') {
        if segment == ".." {
            return Err((
                StatusCode::FORBIDDEN,
                "Path traversal is not allowed".to_string(),
            ));
        }
    }
    for segment in decoded.split('\\') {
        if segment == ".." {
            return Err((
                StatusCode::FORBIDDEN,
                "Path traversal is not allowed".to_string(),
            ));
        }
    }

    let canonical_root = team_path.canonicalize().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to resolve team path".to_string(),
        )
    })?;

    let target = team_path.join(&decoded);

    // For write operations, canonicalize the parent directory (which must exist)
    let parent = target.parent().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid file path".to_string(),
        )
    })?;

    if !parent.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Parent directory not found: {}", relative),
        ));
    }

    let canonical_parent = parent.canonicalize().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to resolve parent path".to_string(),
        )
    })?;

    if !canonical_parent.starts_with(&canonical_root) {
        return Err((
            StatusCode::FORBIDDEN,
            "Path traversal is not allowed".to_string(),
        ));
    }

    // Return the canonical parent joined with the filename
    let filename = target.file_name().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid file path".to_string(),
        )
    })?;

    Ok(canonical_parent.join(filename))
}

/// Simple percent-decoding for path segments.
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &input[i + 1..i + 3],
                16,
            ) {
                result.push(byte as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

// ── Handlers ──────────────────────────────────────────

fn resolve_team_path(
    state: &WebState,
    team_name: &str,
) -> Result<PathBuf, (StatusCode, String)> {
    let cfg = config::load_from(&state.config_path).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;
    let team = cfg
        .teams
        .iter()
        .find(|t| t.name == team_name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Team '{}' not found", team_name),
            )
        })?;
    Ok(team.path.join("team"))
}

fn do_read_file(
    state: &WebState,
    team_name: &str,
    file_path: &str,
) -> Result<FileReadResponse, (StatusCode, String)> {
    let team_path = resolve_team_path(state, team_name)?;
    let resolved = safe_resolve(&team_path, file_path)?;

    if !resolved.is_file() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{} is not a file", file_path),
        ));
    }

    let content = fs::read_to_string(&resolved).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read file: {}", e),
        )
    })?;

    let content_type = detect_content_type(file_path);

    let last_modified = fs::metadata(&resolved)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        })
        .unwrap_or_default();

    Ok(FileReadResponse {
        path: file_path.to_string(),
        content,
        content_type,
        last_modified,
    })
}

async fn do_write_file(
    state: &WebState,
    team_name: &str,
    file_path: &str,
    content: &str,
) -> Result<FileWriteResponse, (StatusCode, String)> {
    let team_path = resolve_team_path(state, team_name)?;
    let resolved = safe_resolve_for_write(&team_path, file_path)?;

    // Write file to disk
    fs::write(&resolved, content).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write file: {}", e),
        )
    })?;

    // Git add + commit
    let commit_sha = git_commit(&team_path, file_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Git commit failed: {}", e),
        )
    })?;

    Ok(FileWriteResponse {
        ok: true,
        path: file_path.to_string(),
        commit_sha,
    })
}

fn do_list_tree(
    state: &WebState,
    team_name: &str,
    rel_path: &str,
) -> Result<TreeResponse, (StatusCode, String)> {
    let team_path = resolve_team_path(state, team_name)?;

    let dir_path = if rel_path.is_empty() {
        // For root listing, just use team_path directly (no safe_resolve needed)
        team_path.clone()
    } else {
        safe_resolve(&team_path, rel_path)?
    };

    if !dir_path.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{} is not a directory", rel_path),
        ));
    }

    let mut entries: Vec<TreeEntry> = fs::read_dir(&dir_path)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list directory: {}", e),
            )
        })?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // Skip hidden files/dirs (like .git, .gitkeep)
            !name.starts_with('.')
        })
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.path().is_dir();
            let entry_path = if rel_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", rel_path, name)
            };
            TreeEntry {
                name,
                entry_type: if is_dir {
                    "directory".to_string()
                } else {
                    "file".to_string()
                },
                path: entry_path,
            }
        })
        .collect();

    // Sort: directories first, then files, alphabetical within each group
    entries.sort_by(|a, b| {
        let a_is_dir = a.entry_type == "directory";
        let b_is_dir = b.entry_type == "directory";
        b_is_dir.cmp(&a_is_dir).then(a.name.cmp(&b.name))
    });

    Ok(TreeResponse {
        path: if rel_path.is_empty() {
            ".".to_string()
        } else {
            rel_path.to_string()
        },
        entries,
    })
}

fn detect_content_type(path: &str) -> String {
    match path.rsplit('.').next().unwrap_or("") {
        "yml" | "yaml" => "yaml".to_string(),
        "md" => "markdown".to_string(),
        "json" => "json".to_string(),
        _ => "text".to_string(),
    }
}

async fn git_commit(repo_path: &Path, file_path: &str) -> anyhow::Result<String> {
    // git add <file>
    let add_output = tokio::process::Command::new("git")
        .args(["-C", &repo_path.to_string_lossy(), "add", file_path])
        .output()
        .await?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        anyhow::bail!("git add failed: {}", stderr);
    }

    // git commit -m "console: update <path>"
    let msg = format!("console: update {}", file_path);
    let commit_output = tokio::process::Command::new("git")
        .args([
            "-C",
            &repo_path.to_string_lossy(),
            "commit",
            "-m",
            &msg,
        ])
        .output()
        .await?;

    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        anyhow::bail!("git commit failed: {}", stderr);
    }

    // git rev-parse HEAD to get commit SHA
    let rev_output = tokio::process::Command::new("git")
        .args(["-C", &repo_path.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .await?;

    if !rev_output.status.success() {
        let stderr = String::from_utf8_lossy(&rev_output.stderr);
        anyhow::bail!("git rev-parse failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&rev_output.stdout).trim().to_string())
}

// ── Types ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct FileWriteRequest {
    pub content: String,
}

#[derive(Deserialize)]
pub struct TreeQuery {
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileReadResponse {
    pub path: String,
    pub content: String,
    pub content_type: String,
    pub last_modified: String,
}

#[derive(Debug, Serialize)]
pub struct FileWriteResponse {
    pub ok: bool,
    pub path: String,
    pub commit_sha: String,
}

#[derive(Serialize)]
pub struct TreeEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct TreeResponse {
    pub path: String,
    pub entries: Vec<TreeEntry>,
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::web::web_router;
    use std::sync::Arc;

    fn setup_fixture_team(tmp: &Path) -> PathBuf {
        let team_dir = tmp.join("my-team");
        let team_repo = team_dir.join("team");
        let fixture_base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.agents/planning/2026-03-22-console-web-ui/fixture-gen/fixtures/team-repo");
        copy_dir_recursive(&fixture_base, &team_repo);
        team_dir
    }

    fn copy_dir_recursive(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).unwrap();
            }
        }
    }

    fn git_init(path: &Path) {
        std::process::Command::new("git")
            .args(["-C", &path.to_string_lossy(), "init"])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "config",
                "user.email",
                "test@test.com",
            ])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "config",
                "user.name",
                "Test",
            ])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", &path.to_string_lossy(), "add", "."])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "commit",
                "-m",
                "initial",
            ])
            .output()
            .unwrap();
    }

    fn test_app(config_path: PathBuf) -> axum::Router {
        let state = super::super::state::WebState {
            config_path: Arc::new(config_path),
        };
        web_router(state)
    }

    fn write_config(
        config_path: &Path,
        team_name: &str,
        team_path: &Path,
        profile_name: &str,
        github_repo: &str,
    ) {
        let cfg = config::BotminterConfig {
            workzone: team_path.parent().unwrap().to_path_buf(),
            default_team: Some(team_name.to_string()),
            teams: vec![config::TeamEntry {
                name: team_name.to_string(),
                path: team_path.to_path_buf(),
                profile: profile_name.to_string(),
                github_repo: github_repo.to_string(),
                credentials: config::Credentials::default(),
                coding_agent: None,
                project_number: None,
                bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
            }],
            vms: Vec::new(),
            keyring_collection: None,
        };
        config::save_to(config_path, &cfg).unwrap();
    }

    #[tokio::test]
    async fn read_file_returns_content_and_type() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/files/botminter.yml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let file: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(file["path"], "botminter.yml");
        assert_eq!(file["content_type"], "yaml");
        assert!(file["content"].as_str().unwrap().contains("statuses:"));
        assert!(file["last_modified"].is_string());
    }

    #[tokio::test]
    async fn read_file_returns_markdown_type() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/files/PROCESS.md")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let file: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(file["content_type"], "markdown");
    }

    #[tokio::test]
    async fn read_file_returns_404_for_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/files/nonexistent.yml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn path_traversal_dot_dot_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/files/../../../etc/passwd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn path_traversal_encoded_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        // %2e = '.' so %2e%2e = '..'
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/files/%2e%2e/etc/passwd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn path_traversal_absolute_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        // We can't put /etc/passwd in a URL path param easily since axum
        // treats the leading / differently. Instead test via the handler directly.
        let state = super::super::state::WebState {
            config_path: Arc::new(config_path),
        };
        let result = do_read_file(&state, "my-team", "/etc/passwd");
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn path_traversal_symlink_escape_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        // Create a symlink inside team repo that points outside
        let link_path = team_path.join("evil-link");
        std::os::unix::fs::symlink("/etc", &link_path).unwrap();

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/files/evil-link/passwd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should be 403 (symlink escapes repo root) or 404 (if intermediate resolution fails)
        assert!(
            resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::NOT_FOUND,
            "Expected 403 or 404, got {}",
            resp.status()
        );

        // Clean up symlink
        fs::remove_file(&link_path).ok();
    }

    // ── Path traversal tests for PUT (write) endpoint ──

    #[tokio::test]
    async fn write_path_traversal_dot_dot_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        git_init(&team_path.join("team"));
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/teams/my-team/files/../../../etc/shadow")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "content": "pwned" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn write_path_traversal_encoded_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        git_init(&team_path.join("team"));
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/teams/my-team/files/%2e%2e/etc/shadow")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "content": "pwned" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn write_path_traversal_absolute_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        git_init(&team_path.join("team"));
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let state = super::super::state::WebState {
            config_path: Arc::new(config_path),
        };
        let result = do_write_file(&state, "my-team", "/etc/shadow", "pwned").await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // ── Path traversal tests for tree listing endpoint ──

    #[tokio::test]
    async fn tree_path_traversal_dot_dot_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/tree?path=../../etc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn tree_path_traversal_encoded_returns_403() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/tree?path=%2e%2e/%2e%2e/etc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn write_file_and_git_commit_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        git_init(&team_path.join("team"));
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path.clone());
        let new_content = "# Updated\nNew content here\n";
        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/teams/my-team/files/PROCESS.md")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "content": new_content }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["path"], "PROCESS.md");
        assert!(
            result["commit_sha"].as_str().unwrap().len() >= 7,
            "commit_sha should be a full SHA"
        );

        // Verify file content on disk (team repo is at team_dir/team/)
        let team_repo_path = team_path.join("team");
        let on_disk = fs::read_to_string(team_repo_path.join("PROCESS.md")).unwrap();
        assert_eq!(on_disk, new_content);

        // Verify git log contains the commit message
        let log_output = std::process::Command::new("git")
            .args(["-C", &team_repo_path.to_string_lossy(), "log", "--oneline", "-1"])
            .output()
            .unwrap();
        let log_line = String::from_utf8_lossy(&log_output.stdout);
        assert!(
            log_line.contains("console: update PROCESS.md"),
            "Git log should contain commit message, got: {}",
            log_line
        );

        // Verify we can read the file back
        let app2 = test_app(config_path);
        let resp2 = app2
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/files/PROCESS.md")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp2.status(), StatusCode::OK);
        let body2 = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let file: serde_json::Value = serde_json::from_slice(&body2).unwrap();
        assert_eq!(file["content"], new_content);
    }

    #[tokio::test]
    async fn tree_listing_returns_sorted_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/tree")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let tree: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(tree["path"], ".");
        let entries = tree["entries"].as_array().unwrap();
        assert!(!entries.is_empty(), "Root should have entries");

        // Verify each entry has required fields
        for entry in entries {
            assert!(entry["name"].is_string());
            assert!(
                entry["type"] == "file" || entry["type"] == "directory",
                "type must be file or directory"
            );
            assert!(entry["path"].is_string());
        }

        // Verify directories come before files
        let mut seen_file = false;
        for entry in entries {
            if entry["type"] == "file" {
                seen_file = true;
            } else if entry["type"] == "directory" && seen_file {
                panic!("Directories should come before files in the listing");
            }
        }

        // Should include known entries from fixture
        let names: Vec<&str> = entries
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"members"), "Should contain members dir");
        assert!(
            names.contains(&"botminter.yml"),
            "Should contain botminter.yml"
        );
    }

    #[tokio::test]
    async fn tree_listing_with_path_param() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/tree?path=members")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let tree: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(tree["path"], "members");
        let entries = tree["entries"].as_array().unwrap();

        // Should list member directories dynamically (team repo at team_dir/team/)
        let expected_count = fs::read_dir(team_path.join("team").join("members"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count();
        assert_eq!(entries.len(), expected_count);

        // Each entry should have path prefixed with members/
        for entry in entries {
            let path = entry["path"].as_str().unwrap();
            assert!(
                path.starts_with("members/"),
                "Entry path should be prefixed: {}",
                path
            );
        }
    }

    #[tokio::test]
    async fn tree_returns_404_for_unknown_team() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".botminter").join("config.yml");
        let cfg = config::BotminterConfig {
            workzone: tmp.path().to_path_buf(),
            default_team: None,
            teams: Vec::new(),
            vms: Vec::new(),
            keyring_collection: None,
        };
        config::save_to(&config_path, &cfg).unwrap();

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/nonexistent/tree")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
