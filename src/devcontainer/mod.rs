use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use json_comments::CommentSettings;
use serde::Deserialize;
use serde_json::Value;

pub(crate) const PRIMARY_CONFIG: &str = ".devcontainer/devcontainer.json";
pub(crate) const ROOT_CONFIG: &str = ".devcontainer.json";
pub(crate) const NESTED_CONFIG_PATTERN: &str = ".devcontainer/<folder>/devcontainer.json";

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
    pub(crate) config_path: String,
    pub(crate) persist_selection: bool,
    pub(crate) config: DevContainer,
}

#[derive(Clone, Copy, Debug)]
enum Selection {
    NonInteractive,
    Index(usize),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ConfigChoice {
    pub(crate) path: String,
    pub(crate) persist: bool,
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

pub(crate) fn discover_local(
    root: &Path,
    requested: Option<&str>,
    persisted: Option<&str>,
) -> Result<LocalProject> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace source {}", root.display()))?;
    let candidates = local_config_candidates(&root)?;
    ensure_config_candidates(&candidates, &root.display().to_string())?;
    let choice = choose_config(
        &root.display().to_string(),
        &candidates,
        requested,
        persisted,
    )?;
    let path = root.join(&choice.path);
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
        config_path: choice.path,
        persist_selection: choice.persist,
        config,
    })
}

pub(crate) fn config_relative_paths() -> [&'static str; 2] {
    [PRIMARY_CONFIG, ROOT_CONFIG]
}

pub(crate) fn ensure_config_candidates(candidates: &[String], source: &str) -> Result<()> {
    ensure!(
        !candidates.is_empty(),
        "no Dev Container configuration found in workspace source {source:?}; checked {PRIMARY_CONFIG}, {ROOT_CONFIG}, and {NESTED_CONFIG_PATTERN}"
    );
    Ok(())
}

pub(crate) fn choose_config(
    source: &str,
    candidates: &[String],
    requested: Option<&str>,
    persisted: Option<&str>,
) -> Result<ConfigChoice> {
    if requested.is_some()
        || persisted.is_some()
        || candidates.len() <= 1
        || candidates
            .first()
            .is_some_and(|path| is_standard_path(path))
        || !io::stdin().is_terminal()
    {
        return select_config(
            source,
            candidates,
            requested,
            persisted,
            Selection::NonInteractive,
        );
    }

    let mut stdout = io::stdout().lock();
    writeln!(
        stdout,
        "Multiple Dev Container configurations found in {source:?}:\n"
    )?;
    for (index, path) in candidates.iter().enumerate() {
        writeln!(stdout, "  {}. {path}", index + 1)?;
    }
    write!(stdout, "\nSelect configuration [1/{}]: ", candidates.len())?;
    stdout.flush()?;
    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .context("failed to read Dev Container configuration selection")?;
    let selected = response
        .trim()
        .parse::<usize>()
        .context("Dev Container configuration selection must be a number")?;
    select_config(source, candidates, None, None, Selection::Index(selected))
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

fn local_config_candidates(root: &Path) -> Result<Vec<String>> {
    let mut candidates = config_relative_paths()
        .into_iter()
        .filter(|relative| root.join(relative).is_file())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let nested_root = root.join(".devcontainer");
    if !nested_root.is_dir() {
        return Ok(candidates);
    }

    let entries = fs::read_dir(&nested_root).with_context(|| {
        format!(
            "failed to inspect Dev Container configurations in {}",
            nested_root.display()
        )
    })?;
    let mut nested = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect Dev Container configurations in {}",
                nested_root.display()
            )
        })?;
        if entry.path().join("devcontainer.json").is_file() {
            let folder = entry.file_name().into_string().map_err(|_| {
                anyhow::anyhow!(
                    "Dev Container configuration folder in {} is not valid UTF-8",
                    nested_root.display()
                )
            })?;
            nested.push(format!(".devcontainer/{folder}/devcontainer.json"));
        }
    }
    nested.sort();
    candidates.extend(nested);
    Ok(candidates)
}

fn select_config(
    source: &str,
    candidates: &[String],
    requested: Option<&str>,
    persisted: Option<&str>,
    selection: Selection,
) -> Result<ConfigChoice> {
    ensure_config_candidates(candidates, source)?;
    if let Some(requested) = requested {
        let path = candidates
            .iter()
            .find(|candidate| candidate.as_str() == requested)
            .with_context(|| selection_error("requested", requested, source, candidates))?;
        return Ok(ConfigChoice {
            path: path.clone(),
            persist: true,
        });
    }
    if let Some(persisted) = persisted {
        let path = candidates
            .iter()
            .find(|candidate| candidate.as_str() == persisted)
            .with_context(|| selection_error("saved", persisted, source, candidates))?;
        return Ok(ConfigChoice {
            path: path.clone(),
            persist: true,
        });
    }
    if let Some(path) = candidates.first()
        && (is_standard_path(path) || candidates.len() == 1)
    {
        return Ok(ConfigChoice {
            path: path.clone(),
            persist: false,
        });
    }
    match selection {
        Selection::NonInteractive => bail!(
            "multiple Dev Container configurations found in {source:?}; use --config <path> to select one (available: {})",
            candidates.join(", ")
        ),
        Selection::Index(index) => {
            ensure!(
                index > 0,
                "Dev Container configuration selection must be between 1 and {}",
                candidates.len()
            );
            let path = candidates.get(index - 1).with_context(|| {
                format!(
                    "Dev Container configuration selection must be between 1 and {}",
                    candidates.len()
                )
            })?;
            Ok(ConfigChoice {
                path: path.clone(),
                persist: true,
            })
        }
    }
}

fn is_standard_path(path: &str) -> bool {
    matches!(path, PRIMARY_CONFIG | ROOT_CONFIG)
}

fn selection_error(kind: &str, path: &str, source: &str, candidates: &[String]) -> String {
    format!(
        "{kind} Dev Container configuration {path:?} was not found in workspace source {source:?}; use --config <path> to select one (available: {})",
        candidates.join(", ")
    )
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

    use super::{
        DevContainer, LifecycleCommand, Selection, discover_local, select_config, substitute,
    };

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
            let project = discover_local(&directory, None, None)?;
            assert_eq!(project.config.image.as_deref(), Some("alpine"));
            fs::remove_dir_all(directory)?;
        }
        Ok(())
    }

    #[test]
    fn discovers_nested_configs_with_standard_precedence() -> anyhow::Result<()> {
        let directory = test_directory("nested-precedence");
        let primary = directory.join(".devcontainer/devcontainer.json");
        let root = directory.join(".devcontainer.json");
        let nested = directory.join(".devcontainer/rust/devcontainer.json");
        for path in [&primary, &root, &nested] {
            fs::create_dir_all(path.parent().context("test path has no parent")?)?;
            fs::write(path, r#"{"image":"alpine"}"#)?;
        }

        let project = discover_local(&directory, None, None)?;
        assert_eq!(project.config_path, ".devcontainer/devcontainer.json");
        assert!(!project.persist_selection);

        let selected = discover_local(
            &directory,
            Some(".devcontainer/rust/devcontainer.json"),
            None,
        )?;
        assert_eq!(selected.config_path, ".devcontainer/rust/devcontainer.json");
        assert!(selected.persist_selection);
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn automatically_selects_one_nested_config_and_ignores_deeper_configs() -> anyhow::Result<()> {
        let directory = test_directory("single-nested");
        let nested = directory.join(".devcontainer/rust/devcontainer.json");
        let deeper = directory.join(".devcontainer/rust/experimental/devcontainer.json");
        for path in [&nested, &deeper] {
            fs::create_dir_all(path.parent().context("test path has no parent")?)?;
            fs::write(path, r#"{"image":"alpine"}"#)?;
        }

        let project = discover_local(&directory, None, None)?;
        assert_eq!(project.config_path, ".devcontainer/rust/devcontainer.json");
        assert!(!project.persist_selection);
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn selects_and_persists_one_of_multiple_nested_configs() -> anyhow::Result<()> {
        let candidates = vec![
            ".devcontainer/go/devcontainer.json".to_owned(),
            ".devcontainer/rust/devcontainer.json".to_owned(),
        ];
        let selected = select_config("/workspace", &candidates, None, None, Selection::Index(2))?;
        assert_eq!(selected.path, ".devcontainer/rust/devcontainer.json");
        assert!(selected.persist);
        let saved = select_config(
            "/workspace",
            &candidates,
            None,
            Some(".devcontainer/go/devcontainer.json"),
            Selection::NonInteractive,
        )?;
        assert_eq!(saved.path, ".devcontainer/go/devcontainer.json");
        assert!(saved.persist);
        assert!(
            select_config(
                "/workspace",
                &candidates,
                None,
                None,
                Selection::NonInteractive
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn rejects_missing_requested_saved_and_any_config() -> anyhow::Result<()> {
        let candidates = vec![".devcontainer/rust/devcontainer.json".to_owned()];
        assert!(
            select_config(
                "/workspace",
                &candidates,
                Some(".devcontainer/go/devcontainer.json"),
                None,
                Selection::NonInteractive
            )
            .is_err()
        );
        assert!(
            select_config(
                "/workspace",
                &candidates,
                None,
                Some(".devcontainer/go/devcontainer.json"),
                Selection::NonInteractive
            )
            .is_err()
        );
        let error = select_config("/workspace", &[], None, None, Selection::NonInteractive)
            .err()
            .context("missing configuration should fail")?;
        assert!(
            error
                .to_string()
                .contains("checked .devcontainer/devcontainer.json")
        );
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
