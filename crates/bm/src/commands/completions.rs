use anyhow::{bail, Result};
use clap_complete::env::Shells;
use clap_complete::Shell;

/// Outputs the dynamic shell registration script.
///
/// When eval'd in the user's shell, the script registers `bm` as the completer
/// binary. Subsequent tab presses invoke `bm` with `COMPLETE=<shell>`, which is
/// intercepted by `CompleteEnv` in `main.rs` to return live candidates.
pub fn run(shell: Shell) -> Result<()> {
    let shells = Shells::builtins();
    let shell_name = shell.to_string();
    let completer = shells.completer(&shell_name);

    match completer {
        Some(c) => {
            c.write_registration("COMPLETE", "bm", "bm", "bm", &mut std::io::stdout())
                .map_err(|e| anyhow::anyhow!(e))?;
        }
        None => {
            bail!(
                "Shell '{}' is not supported for dynamic completions",
                shell_name
            );
        }
    }

    Ok(())
}
