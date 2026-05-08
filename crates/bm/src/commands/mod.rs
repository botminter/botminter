pub mod attach;
pub mod bootstrap;
pub mod brain_run;
pub mod bridge;
pub mod chat;
pub mod credentials;
pub mod debug;
pub mod completions;
pub mod disable;
pub mod enable;
pub mod env;
pub mod meeting;
pub mod daemon;
pub mod fire;
pub mod hire;
pub mod init;
pub mod knowledge;
pub mod members;
pub mod minty;
pub mod profiles;
pub mod profiles_init;
pub mod projects;
pub mod roles;
pub mod start;
pub mod status;
pub mod stop;
pub mod teams;

use anyhow::Result;

use crate::profile::{self, ProfileInitResult};

/// Ensures profiles are initialized, displaying appropriate messages.
/// Used as a guard at the top of commands that require profiles.
///
/// Returns `Ok(())` if profiles are available (existing, newly initialized, or updated).
/// Returns `Err` if the user declined setup or an error occurred.
pub(crate) fn ensure_profiles(config_warning: bool) -> Result<()> {
    let result = profile::ensure_profiles_initialized()?;
    match result {
        ProfileInitResult::AlreadyCurrent => {}
        ProfileInitResult::Initialized { count, path } => {
            eprintln!("Initialized {} profiles in {}", count, path.display());
        }
        ProfileInitResult::Updated { count, path, .. } => {
            eprintln!("Updated {} profiles in {}", count, path.display());
        }
        ProfileInitResult::Declined => {
            eprintln!("Keeping existing profiles");
        }
        ProfileInitResult::SetupDeclined => {
            eprintln!("Run `bm profiles init` to set up profiles.");
            std::process::exit(0);
        }
    }

    // Check config file permissions
    if config_warning {
        if let Ok(path) = crate::config::config_path() {
            if let Some(warning) = crate::config::check_permissions_warning(&path) {
                eprintln!("Warning: {}", warning);
            }
        }
    }

    Ok(())
}
