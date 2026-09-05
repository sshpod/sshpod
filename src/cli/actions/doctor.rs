use crate::podman;
use anyhow::{Context, Result};
use std::io::{self, Write};

pub fn execute() -> Result<()> {
    let version = podman::version()?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "Podman CLI: {version}")
        .and_then(|()| {
            writeln!(
                stdout,
                "Local executable check passed. Runtime and remote connectivity were not checked."
            )
        })
        .context("failed to write doctor output")
}
