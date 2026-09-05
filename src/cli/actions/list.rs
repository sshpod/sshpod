use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::{
    config::Store,
    workspace::{self, ObservedState},
};

/// Print configured workspaces and their state on each provider.
///
/// # Errors
///
/// Returns an error when configuration cannot be read or output cannot be written.
pub fn execute() -> Result<()> {
    let store = Store::discover()?;
    let workspaces = workspace::list(&store)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "WORKSPACE\tPROVIDERS\tSTATUS")?;
    for workspace in workspaces {
        let providers = workspace
            .targets
            .iter()
            .map(|target| target.provider.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let states = workspace
            .targets
            .iter()
            .map(|target| {
                let state = match target.state {
                    ObservedState::Running => "running",
                    ObservedState::Stopped => "stopped",
                    ObservedState::Missing => "not-created",
                    ObservedState::Unreachable => "unreachable",
                    ObservedState::Error => "error",
                };
                format!("{state}:{}", target.provider)
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(stdout, "{}\t{providers}\t{states}", workspace.workspace)?;
    }
    stdout.flush().context("failed to write workspace list")
}
