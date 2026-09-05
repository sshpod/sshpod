use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::provider::{Provider, validate_name};

const CONFIG_VERSION: u32 = 1;

/// Persistent sshpod configuration.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    #[serde(default = "config_version")]
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) providers: BTreeMap<String, Provider>,
    #[serde(default)]
    pub(crate) workspaces: BTreeMap<String, Workspace>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            providers: BTreeMap::new(),
            workspaces: BTreeMap::new(),
        }
    }
}

/// One logical workspace with a target for each configured provider.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Workspace {
    #[serde(default)]
    pub(crate) targets: BTreeMap<String, WorkspaceTarget>,
}

/// Source used for a workspace on one provider.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceTarget {
    pub(crate) source: String,
}

#[derive(Debug)]
pub(crate) struct Store {
    path: PathBuf,
}

impl Store {
    pub(crate) fn discover() -> Result<Self> {
        let path = if let Some(path) = env::var_os("SSHPOD_CONFIG") {
            PathBuf::from(path)
        } else if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
            PathBuf::from(path).join("sshpod/config.toml")
        } else {
            let home = env::var_os("HOME").context(
                "cannot locate sshpod configuration: set HOME, XDG_CONFIG_HOME, or SSHPOD_CONFIG",
            )?;
            PathBuf::from(home).join(".config/sshpod/config.toml")
        };
        Ok(Self { path })
    }

    #[cfg(test)]
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn load(&self) -> Result<Config> {
        if !self.path.exists() {
            return Ok(Config::default());
        }
        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read configuration {}", self.path.display()))?;
        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("invalid configuration {}", self.path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub(crate) fn save(&self, config: &Config) -> Result<()> {
        config.validate()?;
        let parent = self
            .path
            .parent()
            .with_context(|| format!("configuration path {} has no parent", self.path.display()))?;
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create configuration directory {}",
                parent.display()
            )
        })?;
        let contents =
            toml::to_string_pretty(config).context("failed to serialize configuration")?;
        let temporary = self.path.with_extension("toml.tmp");
        fs::write(&temporary, contents).with_context(|| {
            format!(
                "failed to write temporary configuration {}",
                temporary.display()
            )
        })?;
        fs::rename(&temporary, &self.path)
            .with_context(|| format!("failed to replace configuration {}", self.path.display()))
    }
}

impl Config {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == CONFIG_VERSION,
            "unsupported configuration version {}; expected {CONFIG_VERSION}",
            self.version
        );
        for (name, provider) in &self.providers {
            validate_name(name, "provider")?;
            provider.validate(name)?;
        }
        for (name, workspace) in &self.workspaces {
            validate_name(name, "workspace")?;
            ensure!(
                !workspace.targets.is_empty(),
                "workspace {name:?} has no provider targets"
            );
            for (provider, target) in &workspace.targets {
                ensure!(
                    self.providers.contains_key(provider),
                    "workspace {name:?} references unknown provider {provider:?}"
                );
                ensure!(
                    !target.source.trim().is_empty(),
                    "workspace {name:?} has an empty source for provider {provider:?}"
                );
            }
        }
        Ok(())
    }

    pub(crate) fn add_provider(&mut self, name: &str, provider: Provider) -> Result<()> {
        validate_name(name, "provider")?;
        provider.validate(name)?;
        if self.providers.contains_key(name) {
            bail!("provider {name:?} already exists");
        }
        self.providers.insert(name.to_owned(), provider);
        Ok(())
    }

    pub(crate) fn delete_provider(&mut self, name: &str) -> Result<()> {
        ensure!(
            self.providers.contains_key(name),
            "provider {name:?} does not exist"
        );
        for workspace in self.workspaces.values_mut() {
            workspace.targets.remove(name);
        }
        self.workspaces
            .retain(|_, workspace| !workspace.targets.is_empty());
        self.providers.remove(name);
        Ok(())
    }
}

const fn config_version() -> u32 {
    CONFIG_VERSION
}

#[cfg(test)]
mod tests {
    use std::{
        fs, process,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use anyhow::Context;

    use super::{Config, Store};
    use crate::provider::Provider;

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    fn test_path(name: &str) -> std::path::PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("sshpod-config-{}-{id}-{name}", process::id()))
    }

    #[test]
    fn parses_workspace_provider_associations() -> anyhow::Result<()> {
        let path = test_path("parse.toml");
        fs::write(
            &path,
            r#"
version = 1

[providers.local]
type = "local"

[providers.sandbox]
type = "ssh"
host = "sandbox"

[workspaces.permesi.targets.local]
source = "/projects/permesi"

[workspaces.permesi.targets.sandbox]
source = "git@github.com:permesi/permesi.git"
"#,
        )?;
        let mut config = Store::new(path.clone()).load()?;
        assert_eq!(config.providers.len(), 2);
        assert_eq!(
            config
                .workspaces
                .get("permesi")
                .context("expected permesi workspace")?
                .targets
                .len(),
            2
        );
        config.delete_provider("sandbox")?;
        assert_eq!(
            config
                .workspaces
                .get("permesi")
                .context("local target should remain")?
                .targets
                .len(),
            1
        );
        config.delete_provider("local")?;
        assert!(!config.workspaces.contains_key("permesi"));
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn provider_add_save_load_and_delete() -> anyhow::Result<()> {
        let directory = test_path("roundtrip");
        let store = Store::new(directory.join("config.toml"));
        let mut config = Config::default();
        config.add_provider("local", Provider::Local)?;
        config.add_provider(
            "sandbox",
            Provider::Ssh {
                host: "sandbox".to_owned(),
            },
        )?;
        assert!(config.add_provider("local", Provider::Local).is_err());
        store.save(&config)?;

        let mut loaded = store.load()?;
        assert_eq!(loaded.providers.len(), 2);
        loaded.delete_provider("sandbox")?;
        assert!(!loaded.providers.contains_key("sandbox"));
        assert!(loaded.delete_provider("missing").is_err());
        fs::remove_dir_all(directory)?;
        Ok(())
    }
}
