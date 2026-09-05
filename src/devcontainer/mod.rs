use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use json_comments::CommentSettings;
use serde::Deserialize;
use serde_json::Value;

const PRIMARY_CONFIG: &str = ".devcontainer/devcontainer.json";
const ROOT_CONFIG: &str = ".devcontainer.json";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DevContainer {
    #[serde(default, rename = "$schema")]
    _schema: Option<String>,
    #[serde(default)]
    _name: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) build: Option<Build>,
    pub(crate) workspace_folder: Option<String>,
    pub(crate) workspace_mount: Option<String>,
    #[serde(default)]
    pub(crate) mounts: Vec<Value>,
    #[serde(default)]
    pub(crate) container_env: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) remote_env: BTreeMap<String, Option<String>>,
    pub(crate) container_user: Option<String>,
    pub(crate) remote_user: Option<String>,
    pub(crate) initialize_command: Option<LifecycleCommand>,
    pub(crate) on_create_command: Option<LifecycleCommand>,
    pub(crate) update_content_command: Option<LifecycleCommand>,
    pub(crate) post_create_command: Option<LifecycleCommand>,
    pub(crate) post_start_command: Option<LifecycleCommand>,
    #[serde(flatten)]
    unsupported: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Build {
    pub(crate) dockerfile: String,
    #[serde(default = "default_context")]
    pub(crate) context: String,
    #[serde(flatten)]
    unsupported: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum LifecycleCommand {
    Shell(String),
    Direct(Vec<String>),
    Parallel(BTreeMap<String, Value>),
}

#[derive(Debug)]
pub(crate) struct LocalProject {
    pub(crate) root: PathBuf,
    pub(crate) config_directory: PathBuf,
    pub(crate) config: DevContainer,
}

impl DevContainer {
    pub(crate) fn parse(contents: &[u8], source: &str) -> Result<Self> {
        let reader = CommentSettings::c_style().strip_comments(contents);
        let config: Self = serde_json::from_reader(reader)
            .with_context(|| format!("invalid devcontainer config {source}"))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        match (&self.image, &self.build) {
            (Some(image), None) => ensure!(!image.trim().is_empty(), "devcontainer image is empty"),
            (None, Some(build)) => build.validate()?,
            (Some(_), Some(_)) => {
                bail!("devcontainer must specify either `image` or `build`, not both")
            }
            (None, None) => bail!("devcontainer must specify `image` or a simple `build`"),
        }

        if let Some(property) = self.unsupported.keys().next() {
            bail!("devcontainer property {property:?} is not supported yet");
        }
        for (name, command) in self.lifecycle_commands() {
            if let Some(command) = command {
                command.validate(name)?;
            }
        }
        for (key, value) in &self.remote_env {
            ensure!(
                value.is_some(),
                "remoteEnv value for {key:?} cannot be null in this prototype"
            );
        }
        if let Some(mount) = &self.workspace_mount {
            validate_mount(mount, "workspaceMount")?;
        }
        for mount in &self.mounts {
            let mount = mount.as_str().context(
                "devcontainer `mounts` object form is not supported yet; use a bind mount string",
            )?;
            validate_mount(mount, "mounts")?;
        }
        Ok(())
    }

    pub(crate) fn lifecycle_commands(&self) -> [(&'static str, Option<&LifecycleCommand>); 5] {
        [
            ("initializeCommand", self.initialize_command.as_ref()),
            ("onCreateCommand", self.on_create_command.as_ref()),
            ("updateContentCommand", self.update_content_command.as_ref()),
            ("postCreateCommand", self.post_create_command.as_ref()),
            ("postStartCommand", self.post_start_command.as_ref()),
        ]
    }
}

impl Build {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.dockerfile.trim().is_empty(),
            "build.dockerfile is empty"
        );
        ensure!(!self.context.trim().is_empty(), "build.context is empty");
        if let Some(property) = self.unsupported.keys().next() {
            bail!("devcontainer build property {property:?} is not supported yet");
        }
        Ok(())
    }
}

impl LifecycleCommand {
    fn validate(&self, name: &str) -> Result<()> {
        match self {
            Self::Shell(command) => {
                ensure!(!command.trim().is_empty(), "{name} is empty");
                Ok(())
            }
            Self::Direct(command) => {
                ensure!(!command.is_empty(), "{name} array is empty");
                ensure!(
                    command
                        .first()
                        .is_some_and(|program| !program.trim().is_empty()),
                    "{name} program is empty"
                );
                Ok(())
            }
            Self::Parallel(commands) => bail!(
                "{name} object form with {} parallel command(s) is not supported yet",
                commands.len()
            ),
        }
    }
}

pub(crate) fn discover_local(root: &Path) -> Result<LocalProject> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace source {}", root.display()))?;
    let path = config_candidates(&root)
        .into_iter()
        .find(|path| path.is_file())
        .with_context(|| {
            format!(
                "{PRIMARY_CONFIG} not found in workspace source {} (also checked {ROOT_CONFIG})",
                root.display()
            )
        })?;
    let contents = fs::read(&path)
        .with_context(|| format!("failed to read devcontainer config {}", path.display()))?;
    let config = DevContainer::parse(&contents, &path.display().to_string())?;
    let config_directory = path
        .parent()
        .context("devcontainer config path has no parent directory")?
        .to_path_buf();
    Ok(LocalProject {
        root,
        config_directory,
        config,
    })
}

pub(crate) fn config_relative_paths() -> [&'static str; 2] {
    [PRIMARY_CONFIG, ROOT_CONFIG]
}

pub(crate) fn substitute(
    value: &str,
    local_workspace: &str,
    workspace_folder: &str,
    workspace_basename: &str,
) -> Result<String> {
    let mut output = String::new();
    let mut remaining = value;
    while let Some((before, variable)) = remaining.split_once("${") {
        output.push_str(before);
        let (variable, after) = variable
            .split_once('}')
            .with_context(|| format!("unterminated devcontainer variable in {value:?}"))?;
        let replacement = match variable {
            "localWorkspaceFolder" => local_workspace.to_owned(),
            "localWorkspaceFolderBasename" => workspace_basename.to_owned(),
            "containerWorkspaceFolder" => workspace_folder.to_owned(),
            "containerWorkspaceFolderBasename" => workspace_folder
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or(workspace_basename)
                .to_owned(),
            _ => {
                if let Some(name) = variable
                    .strip_prefix("localEnv:")
                    .or_else(|| variable.strip_prefix("env:"))
                {
                    env::var(name).with_context(|| {
                        format!("environment variable {name:?} used by devcontainer is not set")
                    })?
                } else {
                    bail!("devcontainer variable ${{{variable}}} is not supported yet");
                }
            }
        };
        output.push_str(&replacement);
        remaining = after;
    }
    output.push_str(remaining);
    Ok(output)
}

fn config_candidates(root: &Path) -> [PathBuf; 2] {
    [root.join(PRIMARY_CONFIG), root.join(ROOT_CONFIG)]
}

fn validate_mount(mount: &str, property: &str) -> Result<()> {
    let mut mount_type = None;
    let mut source = false;
    let mut target = false;
    for part in mount.split(',') {
        if let Some((key, value)) = part.split_once('=') {
            match key.trim() {
                "type" => mount_type = Some(value.trim()),
                "source" | "src" => source = !value.trim().is_empty(),
                "target" | "dst" | "destination" => target = !value.trim().is_empty(),
                _ => {}
            }
        }
    }
    ensure!(
        mount_type == Some("bind"),
        "{property} only supports `type=bind` in this prototype"
    );
    ensure!(source, "{property} bind mount requires `source` or `src`");
    ensure!(
        target,
        "{property} bind mount requires `target`, `dst`, or `destination`"
    );
    Ok(())
}

fn default_context() -> String {
    ".".to_owned()
}

#[cfg(test)]
mod tests {
    use std::{
        fs, process,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use anyhow::Context;

    use super::{DevContainer, LifecycleCommand, discover_local, substitute};

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    fn test_directory(name: &str) -> std::path::PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("sshpod-devcontainer-{}-{id}-{name}", process::id()))
    }

    #[test]
    fn parses_jsonc_image_configuration() -> anyhow::Result<()> {
        let config = DevContainer::parse(
            br#"{
                // A common image-only configuration.
                "name": "example",
                "image": "docker.io/library/alpine:latest",
                "containerEnv": { "MODE": "dev" },
                "postCreateCommand": ["sh", "-c", "echo ready"]
            }"#,
            "test.json",
        )?;
        assert_eq!(
            config.image.as_deref(),
            Some("docker.io/library/alpine:latest")
        );
        assert_eq!(
            config.container_env.get("MODE").map(String::as_str),
            Some("dev")
        );
        assert!(matches!(
            config.post_create_command,
            Some(LifecycleCommand::Direct(_))
        ));
        Ok(())
    }

    #[test]
    fn parses_simple_dockerfile_build() -> anyhow::Result<()> {
        let config = DevContainer::parse(
            br#"{ "build": { "dockerfile": "Dockerfile", "context": ".." } }"#,
            "test.json",
        )?;
        let build = config.build.context("expected build configuration")?;
        assert_eq!(build.dockerfile, "Dockerfile");
        assert_eq!(build.context, "..");
        Ok(())
    }

    #[test]
    fn rejects_unsupported_properties_and_lifecycle_objects() {
        assert!(DevContainer::parse(br#"{"image":"alpine","features":{}}"#, "test").is_err());
        assert!(
            DevContainer::parse(
                br#"{"image":"alpine","postStartCommand":{"a":"true"}}"#,
                "test"
            )
            .is_err()
        );
    }

    #[test]
    fn discovers_both_config_locations() -> anyhow::Result<()> {
        for relative in [".devcontainer/devcontainer.json", ".devcontainer.json"] {
            let directory = test_directory(&relative.replace('/', "-"));
            let path = directory.join(relative);
            fs::create_dir_all(path.parent().context("test path has no parent")?)?;
            fs::write(&path, r#"{"image":"alpine"}"#)?;
            let project = discover_local(&directory)?;
            assert_eq!(project.config.image.as_deref(), Some("alpine"));
            fs::remove_dir_all(directory)?;
        }
        Ok(())
    }

    #[test]
    fn substitutes_supported_variables() -> anyhow::Result<()> {
        assert_eq!(
            substitute(
                "${localWorkspaceFolder}/x:${containerWorkspaceFolder}",
                "/source",
                "/workspaces/demo",
                "demo"
            )?,
            "/source/x:/workspaces/demo"
        );
        assert!(substitute("${unknown}", "/source", "/workspace", "demo").is_err());
        Ok(())
    }
}
