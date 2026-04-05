mod common;
mod credential;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use anyhow::{bail, Result};

use super::Formation;

/// Creates a local formation appropriate for the current platform.
///
/// Linux and macOS both use the same local-process formation surface, with
/// platform-specific credential backend details handled under `credential`.
pub fn create_local_formation(team_name: &str) -> Result<Box<dyn Formation>> {
    #[cfg(target_os = "linux")]
    {
        return Ok(Box::new(linux::LinuxLocalFormation::new(team_name)));
    }

    #[cfg(target_os = "macos")]
    {
        return Ok(Box::new(macos::MacosLocalFormation::new(team_name)));
    }

    #[allow(unreachable_code)]
    {
        let _ = team_name;
        bail!("Local formation is not supported on this platform")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_local_formation_returns_formation_on_supported_platforms() {
        if !(cfg!(target_os = "linux") || cfg!(target_os = "macos")) {
            return;
        }
        let formation = create_local_formation("my-team").unwrap();
        assert_eq!(formation.name(), "local");
    }

    #[test]
    fn create_local_formation_returns_boxed_dyn_formation() {
        if !(cfg!(target_os = "linux") || cfg!(target_os = "macos")) {
            return;
        }
        let formation: Box<dyn Formation> = create_local_formation("my-team").unwrap();
        assert_eq!(formation.name(), "local");
    }
}
