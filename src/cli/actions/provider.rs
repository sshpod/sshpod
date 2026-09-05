use std::io::{self, Write};

use anyhow::{Context, Result, bail};

use crate::{config::Store, provider::Provider};

/// Print configured providers.
///
/// # Errors
///
/// Returns an error when configuration cannot be read or output cannot be written.
pub fn list() -> Result<()> {
    let store = Store::discover()?;
    let config = store.load()?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "NAME\tTYPE\tHOST")?;
    for (name, provider) in config.providers {
        writeln!(
            stdout,
            "{name}\t{}\t{}",
            provider.kind(),
            provider.host().unwrap_or("-")
        )?;
    }
    stdout.flush().context("failed to write provider list")
}

/// Add a provider to persistent configuration.
///
/// # Errors
///
/// Returns an error for invalid provider options or when configuration cannot be saved.
pub fn add(name: &str, provider_type: &str, host: Option<&str>) -> Result<()> {
    let provider = match (provider_type, host) {
        ("local", None) => Provider::Local,
        ("local", Some(_)) => bail!("local provider {name:?} does not accept --host"),
        ("ssh", Some(host)) => Provider::Ssh {
            host: host.to_owned(),
        },
        ("ssh", None) => bail!("SSH provider {name:?} requires --host <SSH_CONFIG_HOST>"),
        (kind, _) => bail!("unsupported provider type {kind:?}; expected local or ssh"),
    };
    let store = Store::discover()?;
    let mut config = store.load()?;
    config.add_provider(name, provider)?;
    store.save(&config)?;
    writeln!(
        io::stdout().lock(),
        "Added provider {name:?} to {}",
        store.path().display()
    )
    .context("failed to write provider result")
}

/// Delete a provider and its workspace target bindings from persistent configuration.
///
/// # Errors
///
/// Returns an error when the provider is missing or configuration cannot be saved.
pub fn delete(name: &str) -> Result<()> {
    let store = Store::discover()?;
    let mut config = store.load()?;
    config.delete_provider(name)?;
    store.save(&config)?;
    writeln!(
        io::stdout().lock(),
        "Deleted provider {name:?}; existing containers and source directories were not deleted"
    )
    .context("failed to write provider result")
}
