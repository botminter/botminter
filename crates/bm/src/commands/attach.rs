use std::io::IsTerminal;

use anyhow::{bail, Result};

use crate::config;
use crate::formation;
use crate::formation::lima::{Lima, VmStatus};

/// Handles `bm attach [-t <team>]`.
///
/// For v2 teams (with formations dir), delegates to `formation.shell()`.
/// For v1 teams (Lima VMs), uses legacy Lima exec_shell behavior.
pub fn run(team: Option<&str>) -> Result<()> {
    let cfg = config::load_or_default();

    // Try v2 formation path first: if the team has a formations dir,
    // delegate to formation.shell().
    if let Ok(team_entry) = config::resolve_team(&cfg, team) {
        let team_repo = team_entry.path.join("team");
        if let Ok(Some(_)) = formation::resolve_formation(&team_repo, None) {
            let local_formation = formation::create_local_formation(&team_entry.name)?;
            return local_formation.shell();
        }
    }

    // v1 team (no formations dir) — legacy Lima path
    let lima = Lima::check_prerequisites()?;

    let vm_name = match config::resolve_vm(&cfg, team) {
        Ok(name) => name,
        Err(_) if cfg.vms.len() > 1 && std::io::stdin().is_terminal() => {
            select_vm_interactive(&cfg)?
        }
        Err(e) => return Err(e),
    };

    match lima.status(&vm_name)? {
        VmStatus::Running => {}
        VmStatus::Stopped(_) => {
            if std::io::stdin().is_terminal() {
                let start: bool = cliclack::confirm(format!("VM '{}' is not running. Start it?", vm_name))
                    .initial_value(true)
                    .interact()?;
                if !start {
                    bail!("VM '{}' is not running. Start it with `limactl start {}`.", vm_name, vm_name);
                }
            }
            eprintln!("Starting VM '{}'...", vm_name);
            lima.start(&vm_name)?;
            eprintln!("VM '{}' started.", vm_name);
        }
        VmStatus::NotFound => {
            bail!(
                "VM '{}' does not exist. Run `bm env create` to set up the environment, or use `bm runtime create --name {}` for Lima VMs.",
                vm_name, vm_name
            );
        }
    }

    lima.exec_shell(&vm_name)
}

/// Interactive VM selection when multiple VMs are available.
fn select_vm_interactive(cfg: &config::BotminterConfig) -> Result<String> {
    let names: Vec<&str> = cfg.vms.iter().map(|v| v.name.as_str()).collect();
    let selection: String = cliclack::select("Select a VM to attach to")
        .items(
            &names
                .iter()
                .map(|n| (n.to_string(), *n, ""))
                .collect::<Vec<_>>(),
        )
        .interact()?;
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use crate::config::{self, BotminterConfig, Credentials, TeamEntry, VmEntry};
    use std::path::PathBuf;

    fn make_config(
        vms: Vec<VmEntry>,
        teams: Vec<TeamEntry>,
        default_team: Option<String>,
    ) -> BotminterConfig {
        BotminterConfig {
            workzone: PathBuf::from("/tmp/test"),
            default_team,
            teams,
            vms,
            keyring_collection: None,
        }
    }

    fn make_team(name: &str, vm: Option<&str>) -> TeamEntry {
        TeamEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{}", name)),
            profile: "agentic-sdlc-minimal".to_string(),
            github_repo: format!("org/{}", name),
            credentials: Credentials::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: vm.map(String::from),
        }
    }

    #[test]
    fn resolve_vm_no_vms_configured() {
        let cfg = make_config(vec![], vec![], None);
        let result = config::resolve_vm(&cfg, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No VM found"));
        assert!(err.contains("bm env create"));
    }

    #[test]
    fn resolve_vm_single_vm_auto_selects() {
        let cfg = make_config(
            vec![VmEntry {
                name: "bm-solo".to_string(),
            }],
            vec![],
            None,
        );
        let result = config::resolve_vm(&cfg, None).unwrap();
        assert_eq!(result, "bm-solo");
    }

    #[test]
    fn resolve_vm_team_flag_with_vm() {
        let cfg = make_config(
            vec![
                VmEntry { name: "vm-a".to_string() },
                VmEntry { name: "vm-b".to_string() },
            ],
            vec![make_team("alpha", Some("vm-a"))],
            None,
        );
        let result = config::resolve_vm(&cfg, Some("alpha")).unwrap();
        assert_eq!(result, "vm-a");
    }

    #[test]
    fn resolve_vm_team_flag_without_vm_falls_through() {
        let cfg = make_config(
            vec![VmEntry {
                name: "only-vm".to_string(),
            }],
            vec![make_team("beta", None)],
            None,
        );
        let result = config::resolve_vm(&cfg, Some("beta")).unwrap();
        assert_eq!(result, "only-vm");
    }

    #[test]
    fn resolve_vm_default_team_with_vm() {
        let cfg = make_config(
            vec![
                VmEntry { name: "vm-x".to_string() },
                VmEntry { name: "vm-y".to_string() },
            ],
            vec![make_team("default-team", Some("vm-y"))],
            Some("default-team".to_string()),
        );
        let result = config::resolve_vm(&cfg, None).unwrap();
        assert_eq!(result, "vm-y");
    }

    #[test]
    fn resolve_vm_multiple_vms_no_team_errors() {
        let cfg = make_config(
            vec![
                VmEntry { name: "vm-1".to_string() },
                VmEntry { name: "vm-2".to_string() },
            ],
            vec![],
            None,
        );
        let result = config::resolve_vm(&cfg, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Multiple VMs"));
    }

    #[test]
    fn resolve_vm_team_not_found_falls_through() {
        let cfg = make_config(
            vec![VmEntry {
                name: "fallback".to_string(),
            }],
            vec![],
            None,
        );
        let result = config::resolve_vm(&cfg, Some("nonexistent")).unwrap();
        assert_eq!(result, "fallback");
    }
}
