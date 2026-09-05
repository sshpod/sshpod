use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use noyalib::{DuplicateKeyPolicy, ParserConfig, SerializerConfig};
use serde::{Deserialize, Serialize};

use crate::provider::{Provider, validate_name};

const CONFIG_DIRECTORY: &str = "sshpod";
const CONFIG_FILENAME: &str = "config.yaml";

/// Persistent sshpod configuration and workspace state.
#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) default_provider: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) providers: BTreeMap<String, Provider>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) workspaces: BTreeMap<String, Workspace>,
}

/// One logical workspace with a target for each configured provider.
#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) devcontainer: Option<String>,
}

/// A path-bound facade retained for existing workspace and CLI call sites.
#[derive(Debug)]
pub(crate) struct Store {
    path: PathBuf,
}

impl Store {
    pub(crate) fn discover() -> Result<Self> {
        Ok(Self {
            path: config_path_from(
                env::var_os("XDG_CONFIG_HOME").as_deref(),
                env::var_os("HOME").as_deref(),
            )?,
        })
    }

    #[cfg(test)]
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn load(&self) -> Result<Config> {
        Config::load_from(&self.path)
    }

    pub(crate) fn save(&self, config: &Config) -> Result<()> {
        config.save_to(&self.path)
    }
}

impl Config {
    /// Load configuration from the XDG path without creating it.
    pub(crate) fn load() -> Result<Self> {
        Store::discover()?.load()
    }

    /// Load configuration from an explicit path without creating it.
    pub(crate) fn load_from(path: &Path) -> Result<Self> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let parser = ParserConfig::new().duplicate_key_policy(DuplicateKeyPolicy::Error);
        let config: Self = noyalib::from_str_with_config(&contents, &parser)
            .with_context(|| format!("failed to parse sshpod config {}", path.display()))?;
        config
            .validate()
            .with_context(|| format!("invalid sshpod config {}", path.display()))?;
        Ok(config)
    }

    /// Save configuration to the XDG path.
    pub(crate) fn save(&self) -> Result<()> {
        Store::discover()?.save(self)
    }

    /// Validate and atomically save configuration to an explicit path.
    pub(crate) fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()
            .with_context(|| format!("invalid sshpod config {}", path.display()))?;
        let parent = path
            .parent()
            .with_context(|| format!("configuration path {} has no parent", path.display()))?;
        let create_directory = !parent.exists();
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create configuration directory {}",
                parent.display()
            )
        })?;
        set_private_directory_permissions(parent, create_directory)?;

        let serializer = SerializerConfig::new().document_start(true);
        let contents = noyalib::to_string_with_config(self, &serializer)
            .with_context(|| format!("failed to serialize sshpod config {}", path.display()))?;
        let temporary = path.with_extension("yaml.tmp");
        write_private_file(&temporary, contents.as_bytes())?;
        fs::rename(&temporary, path)
            .with_context(|| format!("failed to replace configuration {}", path.display()))
    }

    pub(crate) fn provider(&self, name: &str) -> Option<&Provider> {
        self.providers.get(name)
    }

    pub(crate) fn default_provider(&self) -> Result<Option<(&str, &Provider)>> {
        let Some(name) = self.default_provider.as_deref() else {
            return Ok(None);
        };
        let provider = self
            .provider(name)
            .with_context(|| format!("default provider {name:?} does not exist"))?;
        Ok(Some((name, provider)))
    }

    pub(crate) fn validate(&self) -> Result<()> {
        for (name, provider) in &self.providers {
            validate_name(name, "provider")?;
            provider.validate(name)?;
        }
        self.default_provider()?;
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
                if let Some(devcontainer) = &target.devcontainer {
                    ensure!(
                        !devcontainer.trim().is_empty(),
                        "workspace {name:?} has an empty devcontainer path for provider {provider:?}"
                    );
                }
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
        ensure!(
            self.default_provider.as_deref() != Some(name),
            "cannot delete default provider {name:?}; change defaultProvider first"
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

fn config_path_from(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Result<PathBuf> {
    if let Some(xdg) = xdg_config_home {
        let xdg = Path::new(xdg);
        if !xdg.as_os_str().is_empty() && xdg.is_absolute() {
            return Ok(xdg.join(CONFIG_DIRECTORY).join(CONFIG_FILENAME));
        }
    }
    let home = home
        .context("cannot locate sshpod configuration: set HOME or an absolute XDG_CONFIG_HOME")?;
    ensure!(
        !home.is_empty(),
        "cannot locate sshpod configuration: HOME is empty"
    );
    Ok(PathBuf::from(home)
        .join(".config")
        .join(CONFIG_DIRECTORY)
        .join(CONFIG_FILENAME))
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path, newly_created: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if newly_created {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).with_context(|| {
            format!(
                "failed to secure configuration directory {}",
                path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path, _newly_created: bool) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(contents))
        .with_context(|| format!("failed to write temporary configuration {}", path.display()))
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    fs::write(path, contents)
        .with_context(|| format!("failed to write temporary configuration {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, fs, path::PathBuf, process};

    use anyhow::Context;

    use super::{Config, Store, config_path_from};
    use crate::provider::{Provider, default_local_podman, default_ssh_podman};

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sshpod-config-{}-{name}", process::id()))
    }

    fn parse(name: &str, yaml: &str) -> anyhow::Result<Config> {
        let directory = test_path(name);
        let path = directory.join("config.yaml");
        fs::create_dir_all(&directory)?;
        fs::write(&path, yaml)?;
        let result = Config::load_from(&path);
        fs::remove_dir_all(directory)?;
        result
    }

    fn parse_error(name: &str, yaml: &str) -> anyhow::Result<anyhow::Error> {
        match parse(name, yaml) {
            Ok(_) => anyhow::bail!("config {name} should be invalid"),
            Err(error) => Ok(error),
        }
    }

    #[test]
    fn resolves_xdg_config_home() -> anyhow::Result<()> {
        assert_eq!(
            config_path_from(Some(OsStr::new("/xdg")), Some(OsStr::new("/home/me")))?,
            PathBuf::from("/xdg/sshpod/config.yaml")
        );
        Ok(())
    }

    #[test]
    fn falls_back_to_home_for_missing_empty_or_relative_xdg() -> anyhow::Result<()> {
        let expected = PathBuf::from("/home/me/.config/sshpod/config.yaml");
        for xdg in [None, Some(OsStr::new("")), Some(OsStr::new("relative"))] {
            assert_eq!(
                config_path_from(xdg, Some(OsStr::new("/home/me")))?,
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn parses_minimal_local_provider_with_defaults() -> anyhow::Result<()> {
        let config = parse(
            "minimal-local",
            "defaultProvider: local\nproviders:\n  local:\n    type: local\n",
        )?;
        assert_eq!(
            config.provider("local"),
            Some(&Provider::Local {
                podman: default_local_podman()
            })
        );
        assert_eq!(
            config.default_provider()?.map(|(name, _)| name),
            Some("local")
        );
        Ok(())
    }

    #[test]
    fn parses_minimal_ssh_provider_with_defaults() -> anyhow::Result<()> {
        let config = parse(
            "minimal-ssh",
            "providers:\n  sandbox:\n    type: ssh\n    host: sandbox\n",
        )?;
        assert_eq!(
            config.provider("sandbox"),
            Some(&Provider::Ssh {
                host: "sandbox".to_owned(),
                podman: default_ssh_podman(),
                ssh_args: Vec::new(),
            })
        );
        Ok(())
    }

    #[test]
    fn parses_explicit_podman_and_ssh_arguments() -> anyhow::Result<()> {
        let config = parse(
            "explicit-fields",
            "providers:\n  local:\n    type: local\n    podman: /usr/bin/podman\n  infra-vm:\n    type: ssh\n    host: devops@infra\n    podman: docker\n    sshArgs:\n      - -A\n",
        )?;
        assert_eq!(config.providers.len(), 2);
        assert_eq!(
            config.provider("local").context("missing local")?.podman(),
            "/usr/bin/podman"
        );
        assert_eq!(
            config.provider("infra-vm"),
            Some(&Provider::Ssh {
                host: "devops@infra".to_owned(),
                podman: "docker".to_owned(),
                ssh_args: vec!["-A".to_owned()],
            })
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_default_provider() -> anyhow::Result<()> {
        let error = parse_error(
            "invalid-default",
            "defaultProvider: missing\nproviders:\n  local:\n    type: local\n",
        )?;
        assert!(format!("{error:#}").contains("default provider \"missing\" does not exist"));
        Ok(())
    }

    #[test]
    fn rejects_ssh_provider_without_host() -> anyhow::Result<()> {
        let error = parse_error("missing-host", "providers:\n  sandbox:\n    type: ssh\n")?;
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("host"), "{diagnostic}");
        Ok(())
    }

    #[test]
    fn rejects_empty_provider_names_and_hosts() -> anyhow::Result<()> {
        let name_error = parse_error("empty-name", "providers:\n  \"\":\n    type: local\n")?;
        assert!(format!("{name_error:#}").contains("provider name"));

        let host_error = parse_error(
            "empty-host",
            "providers:\n  sandbox:\n    type: ssh\n    host: \"   \"\n",
        )?;
        assert!(format!("{host_error:#}").contains("requires \"host\""));
        Ok(())
    }

    #[test]
    fn rejects_unknown_provider_fields_types_and_duplicate_names() {
        for (name, yaml) in [
            (
                "unknown-field",
                "providers:\n  local:\n    type: local\n    host: unexpected\n",
            ),
            ("unknown-type", "providers:\n  local:\n    type: plugin\n"),
            (
                "duplicate-provider",
                "providers:\n  local:\n    type: local\n  local:\n    type: local\n",
            ),
        ] {
            assert!(parse(name, yaml).is_err());
        }
    }

    #[test]
    fn reports_malformed_yaml_with_path() -> anyhow::Result<()> {
        let error = parse_error("malformed", "providers: [\n")?;
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("failed to parse sshpod config"));
        assert!(diagnostic.contains("config.yaml"));
        Ok(())
    }

    #[test]
    fn allows_future_top_level_sections() -> anyhow::Result<()> {
        let config = parse(
            "future-section",
            "providers:\n  local:\n    type: local\ndefaults:\n  futureValue: true\n",
        )?;
        assert!(config.provider("local").is_some());
        Ok(())
    }

    #[test]
    fn yaml_round_trip_preserves_providers_and_workspaces() -> anyhow::Result<()> {
        let directory = test_path("round-trip");
        let path = directory.join("config.yaml");
        let mut config = Config {
            default_provider: Some("local".to_owned()),
            ..Config::default()
        };
        config.add_provider(
            "local",
            Provider::Local {
                podman: default_local_podman(),
            },
        )?;
        config.add_provider(
            "sandbox",
            Provider::Ssh {
                host: "sandbox".to_owned(),
                podman: default_ssh_podman(),
                ssh_args: vec!["-A".to_owned()],
            },
        )?;
        config.save_to(&path)?;
        assert!(path.exists());
        assert_eq!(Config::load_from(&path)?, config);
        let yaml = fs::read_to_string(&path)?;
        assert!(yaml.starts_with("---\n"));
        assert!(yaml.contains("defaultProvider: local"));
        assert!(yaml.contains("sshArgs:"));
        assert!(!yaml.contains("podman: podman"));
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn refuses_to_delete_the_default_provider() -> anyhow::Result<()> {
        let mut config = Config {
            default_provider: Some("local".to_owned()),
            ..Config::default()
        };
        config.add_provider(
            "local",
            Provider::Local {
                podman: default_local_podman(),
            },
        )?;
        assert!(config.delete_provider("local").is_err());
        assert!(config.provider("local").is_some());
        Ok(())
    }

    #[test]
    fn missing_config_does_not_create_files_or_directories() -> anyhow::Result<()> {
        let directory = test_path("missing");
        let path = directory.join("sshpod/config.yaml");
        assert_eq!(Config::load_from(&path)?, Config::default());
        assert!(!directory.exists());
        Ok(())
    }

    #[test]
    fn store_loads_workspace_provider_associations() -> anyhow::Result<()> {
        let directory = test_path("workspace");
        let path = directory.join("config.yaml");
        fs::create_dir_all(&directory)?;
        fs::write(
            &path,
            "providers:\n  local:\n    type: local\n  sandbox:\n    type: ssh\n    host: sandbox\nworkspaces:\n  permesi:\n    targets:\n      local:\n        source: /projects/permesi\n      sandbox:\n        source: git@github.com:permesi/permesi.git\n        devcontainer: .devcontainer/rust/devcontainer.json\n",
        )?;
        let mut config = Store::new(path).load()?;
        assert_eq!(config.providers.len(), 2);
        assert_eq!(
            config
                .workspaces
                .get("permesi")
                .context("missing permesi workspace")?
                .targets
                .len(),
            2
        );
        config.delete_provider("sandbox")?;
        assert_eq!(
            config
                .workspaces
                .get("permesi")
                .context("missing permesi workspace")?
                .targets
                .len(),
            1
        );
        fs::remove_dir_all(directory)?;
        Ok(())
    }
}
