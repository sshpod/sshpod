use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::{config::Store, workspace};

/// Stop a workspace on one provider without deleting it.
///
/// # Errors
///
/// Returns an error when configuration cannot be read or Podman cannot stop the container.
pub fn execute(workspace_name: &str, provider: Option<&str>) -> Result<()> {
    let store = Store::discover()?;
    let result = workspace::down(&store, workspace_name, provider)?;
    let state = if result.already_stopped {
        "was already stopped"
    } else {
        "stopped"
    };
    writeln!(
        io::stdout().lock(),
        "Workspace {:?} {state} on provider {:?} ({})",
        result.workspace,
        result.provider,
        result.container
    )
    .context("failed to write workspace result")
}
