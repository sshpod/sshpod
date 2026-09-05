use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};

use crate::{
    config::{Config, Store},
    provider::{Provider, default_local_podman, default_ssh_podman},
};

/// Print configured providers.
///
/// # Errors
///
/// Returns an error when configuration cannot be read or output cannot be written.
pub fn list() -> Result<()> {
    let config = Config::load()?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "NAME\tTYPE\tHOST\tPODMAN\tDEFAULT")?;
    let default_provider = config.default_provider.as_deref();
    for (name, provider) in config.providers {
        writeln!(
            stdout,
            "{name}\t{}\t{}\t{}\t{}",
            provider.kind(),
            provider.host().unwrap_or("-"),
            provider.podman(),
            if default_provider == Some(name.as_str()) {
                "yes"
            } else {
                "-"
            }
        )?;
    }
    stdout.flush().context("failed to write provider list")
}

/// Add a provider to persistent configuration.
///
/// # Errors
///
/// Returns an error for invalid provider options or when configuration cannot be saved.
pub fn add(
    name: &str,
    provider_type: &str,
    host: Option<&str>,
    podman: Option<&str>,
    ssh_args: &[String],
) -> Result<()> {
    let provider = match (provider_type, host) {
        ("local", None) if ssh_args.is_empty() => Provider::Local {
            podman: podman.map_or_else(default_local_podman, PathBuf::from),
        },
        ("local", None) => bail!("local provider {name:?} does not accept --ssh-arg"),
        ("local", Some(_)) => bail!("local provider {name:?} does not accept --host"),
        ("ssh", Some(host)) => Provider::Ssh {
            host: host.to_owned(),
            podman: podman.map_or_else(default_ssh_podman, ToOwned::to_owned),
            ssh_args: ssh_args.to_vec(),
        },
        ("ssh", None) => bail!("SSH provider {name:?} requires --host <SSH_CONFIG_HOST>"),
        (kind, _) => bail!("unsupported provider type {kind:?}; expected local or ssh"),
    };
    let mut config = Config::load()?;
    config.add_provider(name, provider)?;
    config.save()?;
    let path = Store::discover()?;
    writeln!(
        io::stdout().lock(),
        "Added provider {name:?} to {}",
        path.path().display()
    )
    .context("failed to write provider result")
}

/// Delete a provider and its workspace target bindings from persistent configuration.
///
/// # Errors
///
/// Returns an error when the provider is missing or configuration cannot be saved.
pub fn delete(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.delete_provider(name)?;
    config.save()?;
    writeln!(
        io::stdout().lock(),
        "Deleted provider {name:?}; existing containers and source directories were not deleted"
    )
    .context("failed to write provider result")
}
