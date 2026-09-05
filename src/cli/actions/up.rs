use std::{
    env,
    io::{self, Write},
};

use anyhow::{Context, Result};

use crate::{config::Store, workspace};

/// Start a workspace on one provider.
///
/// # Errors
///
/// Returns an error when configuration, source preparation, or Podman execution fails.
pub fn execute(workspace_name: &str, provider: Option<&str>) -> Result<()> {
    let store = Store::discover()?;
    let current_directory = env::current_dir().context("failed to read the current directory")?;
    let result = workspace::up(&store, workspace_name, provider, &current_directory)?;
    let operation = if result.created {
        "created and started"
    } else {
        "started"
    };
    writeln!(
        io::stdout().lock(),
        "Workspace {:?} {operation} on provider {:?} ({})",
        result.workspace,
        result.provider,
        result.container
    )
    .context("failed to write workspace result")
}
