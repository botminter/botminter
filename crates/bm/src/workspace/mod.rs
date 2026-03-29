mod context;
mod repo;
mod robot;
mod sync;
mod team_sync;
mod util;

pub use repo::{
    assemble_workspace_repo_context, create_workspace_repo, GhRemoteOps, RemoteRepoOps,
    RemoteRepoState, WorkspaceRepoParams,
};
pub use context::inject_workspace_context;
pub use robot::{inject_robot_config, inject_robot_enabled, RobotBridgeConfig};
pub use sync::{find_workspace, list_member_dirs, sync_workspace, SyncEvent, SyncResult};
pub use team_sync::{sync_team_workspaces, TeamSyncEvent, TeamSyncParams, TeamSyncResult};
pub use util::{
    workspace_git_branch, workspace_remote_url, workspace_submodule_status, SubmoduleState,
    SubmoduleStatus,
};
