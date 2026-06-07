#[cfg(feature = "console")]
pub mod assets;
pub mod files;
pub mod members;
pub mod overview;
pub mod process;
pub mod sessions;
pub mod state;
pub mod sync;
pub mod teams;

use axum::routing::{get, post};
use axum::Router;

use self::files::{list_tree, read_file, write_file};
use self::members::{get_member, list_members};
use self::overview::team_overview;
use self::process::team_process;
use self::sessions::list_sessions;
use self::state::WebState;
use self::sync::team_sync;
use self::teams::list_teams;

/// Builds the console web API router with all `/api/*` routes.
pub fn web_router(state: WebState) -> Router {
    let router = Router::new()
        .route("/api/teams", get(list_teams))
        .route("/api/teams/{team}/overview", get(team_overview))
        .route("/api/teams/{team}/process", get(team_process))
        .route("/api/teams/{team}/members", get(list_members))
        .route("/api/teams/{team}/members/{name}", get(get_member))
        .route("/api/teams/{team}/tree", get(list_tree))
        .route(
            "/api/teams/{team}/files/{*path}",
            get(read_file).put(write_file),
        )
        .route("/api/teams/{team}/sessions", get(list_sessions))
        .route("/api/teams/{team}/sync", post(team_sync))
        .with_state(state);

    // When built with the `console` feature, serve embedded frontend assets
    // as a fallback for non-API routes (SPA client-side routing).
    #[cfg(feature = "console")]
    let router = router.fallback(assets::serve_embedded_assets);

    router
}
