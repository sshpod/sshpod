mod lifecycle;
mod selection;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail, ensure};

use crate::{
    config::{Config, Store, WorkspaceTarget},
    devcontainer::{self, DevContainer, LocalProject},
    podman::{self, ContainerSpec},
    provider::{Executor, Provider, validate_name},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContainerState {
    Running,
    Stopped,
    Missing,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ObservedState {
    Running,
    Stopped,
    Missing,
    Unreachable,
    Error,
}

#[derive(Debug)]
pub(crate) struct TargetStatus {
    pub(crate) provider: String,
    pub(crate) state: ObservedState,
}

#[derive(Debug)]
pub(crate) struct WorkspaceStatus {
    pub(crate) workspace: String,
    pub(crate) targets: Vec<TargetStatus>,
}

#[derive(Debug)]
pub(crate) struct UpResult {
    pub(crate) workspace: String,
    pub(crate) provider: String,
    pub(crate) container: String,
    pub(crate) created: bool,
}

#[derive(Debug)]
pub(crate) struct DownResult {
    pub(crate) workspace: String,
    pub(crate) provider: String,
    pub(crate) container: String,
    pub(crate) already_stopped: bool,
}

#[derive(Clone, Debug)]
struct ResolvedTarget {
    provider_name: String,
    provider: Provider,
    source: String,
}

#[derive(Debug)]
enum Project {
    Local(LocalProject),
    Remote {
        root: String,
        config_directory: String,
        config: DevContainer,
    },
}

impl Project {
    fn root(&self) -> String {
        match self {
            Self::Local(project) => project.root.display().to_string(),
            Self::Remote { root, .. } => root.clone(),
        }
    }

    fn config_directory(&self) -> String {
        match self {
            Self::Local(project) => project.config_directory.display().to_string(),
            Self::Remote {
                config_directory, ..
            } => config_directory.clone(),
        }
    }

    const fn config(&self) -> &DevContainer {
        match self {
            Self::Local(project) => &project.config,
            Self::Remote { config, .. } => config,
        }
    }
}

pub(crate) fn up(
    store: &Store,
    workspace_name: &str,
    requested_provider: Option<&str>,
    current_directory: &Path,
) -> Result<UpResult> {
    validate_name(workspace_name, "workspace")?;
    let mut config = store.load()?;
    let target = resolve_up_target(
        &mut config,
        workspace_name,
        requested_provider,
        current_directory,
    )?;

    let executor = Executor::new(&target.provider_name, &target.provider);
    podman::check_available(&executor)?;
    let source = prepare_source(&executor, workspace_name, &target)?;
    let project = load_project(&executor, &source)?;
    // Persist only after the source and its devcontainer configuration are valid.
    // Saving before container creation still makes partial Podman failures visible
    // to `list` and recoverable through `down`.
    store.save(&config)?;
    start_project(&executor, workspace_name, &target.provider_name, &project)
}

pub(crate) fn down(
    store: &Store,
    workspace_name: &str,
    requested_provider: Option<&str>,
) -> Result<DownResult> {
    validate_name(workspace_name, "workspace")?;
    let config = store.load()?;
    let target = resolve_configured_target(&config, workspace_name, requested_provider)?;
    let executor = Executor::new(&target.provider_name, &target.provider);
    podman::check_available(&executor)?;
    let container = container_name(workspace_name, &target.provider_name);
    let state = podman::container_status(&executor, &container)?;
    ensure!(
        state != ContainerState::Missing,
        "workspace {workspace_name:?} has no container on provider {:?}",
        target.provider_name
    );
    let already_stopped = state == ContainerState::Stopped;
    if !already_stopped {
        podman::stop(&executor, &container)?;
    }
    Ok(DownResult {
        workspace: workspace_name.to_owned(),
        provider: target.provider_name,
        container,
        already_stopped,
    })
}

pub(crate) fn list(store: &Store) -> Result<Vec<WorkspaceStatus>> {
    let config = store.load()?;
    let mut workspaces = Vec::new();
    for (workspace_name, workspace) in &config.workspaces {
        let mut targets = Vec::new();
        for provider_name in workspace.targets.keys() {
            let state = observe_target(&config, workspace_name, provider_name);
            targets.push(TargetStatus {
                provider: provider_name.clone(),
                state,
            });
        }
        workspaces.push(WorkspaceStatus {
            workspace: workspace_name.clone(),
            targets,
        });
    }
    Ok(workspaces)
}

fn resolve_up_target(
    config: &mut Config,
    workspace_name: &str,
    requested_provider: Option<&str>,
    current_directory: &Path,
) -> Result<ResolvedTarget> {
    if let Some(provider_name) = requested_provider {
        let provider = config
            .providers
            .get(provider_name)
            .with_context(|| format!("provider {provider_name:?} does not exist"))?
            .clone();
        if let Some(target) = config
            .workspaces
            .get(workspace_name)
            .and_then(|workspace| workspace.targets.get(provider_name))
        {
            return Ok(ResolvedTarget {
                provider_name: provider_name.to_owned(),
                provider,
                source: target.source.clone(),
            });
        }
        let source = infer_source(&provider, current_directory)?;
        insert_target(config, workspace_name, provider_name, &source);
        return Ok(ResolvedTarget {
            provider_name: provider_name.to_owned(),
            provider,
            source,
        });
    }

    let candidates = if let Some(workspace) = config.workspaces.get(workspace_name) {
        workspace.targets.keys().cloned().collect::<Vec<_>>()
    } else {
        config.providers.keys().cloned().collect::<Vec<_>>()
    };
    ensure!(
        !candidates.is_empty(),
        "no providers configured; run `sshpod provider add local --type local`"
    );
    let provider_name = selection::choose_provider(workspace_name, &candidates, None)?;
    let provider = config
        .providers
        .get(&provider_name)
        .context("selected provider disappeared from configuration")?
        .clone();
    if let Some(target) = config
        .workspaces
        .get(workspace_name)
        .and_then(|workspace| workspace.targets.get(&provider_name))
    {
        return Ok(ResolvedTarget {
            provider_name,
            provider,
            source: target.source.clone(),
        });
    }
    let source = infer_source(&provider, current_directory)?;
    insert_target(config, workspace_name, &provider_name, &source);
    Ok(ResolvedTarget {
        provider_name,
        provider,
        source,
    })
}

fn resolve_configured_target(
    config: &Config,
    workspace_name: &str,
    requested_provider: Option<&str>,
) -> Result<ResolvedTarget> {
    let workspace = config
        .workspaces
        .get(workspace_name)
        .with_context(|| format!("workspace {workspace_name:?} is not configured"))?;
    let candidates = workspace.targets.keys().cloned().collect::<Vec<_>>();
    let provider_name =
        selection::choose_provider(workspace_name, &candidates, requested_provider)?;
    let provider = config
        .providers
        .get(&provider_name)
        .context("workspace target references a missing provider")?
        .clone();
    let source = workspace
        .targets
        .get(&provider_name)
        .context("selected workspace target disappeared")?
        .source
        .clone();
    Ok(ResolvedTarget {
        provider_name,
        provider,
        source,
    })
}

fn infer_source(provider: &Provider, current_directory: &Path) -> Result<String> {
    match provider {
        Provider::Local => Ok(current_directory
            .canonicalize()
            .with_context(|| {
                format!(
                    "failed to resolve current directory {}",
                    current_directory.display()
                )
            })?
            .display()
            .to_string()),
        Provider::Ssh { .. } => {
            let output = Command::new("git")
                .current_dir(current_directory)
                .args(["config", "--get", "remote.origin.url"])
                .stdin(Stdio::null())
                .output()
                .context("could not execute git to discover the workspace origin")?;
            ensure!(
                output.status.success(),
                "cannot infer an SSH workspace source: current directory has no Git origin; add the workspace target to {}",
                Store::discover()?.path().display()
            );
            let source = String::from_utf8(output.stdout)
                .context("Git origin URL is not valid UTF-8")?
                .trim()
                .to_owned();
            ensure!(!source.is_empty(), "Git origin URL is empty");
            Ok(source)
        }
    }
}

fn insert_target(config: &mut Config, workspace: &str, provider: &str, source: &str) {
    config
        .workspaces
        .entry(workspace.to_owned())
        .or_default()
        .targets
        .insert(
            provider.to_owned(),
            WorkspaceTarget {
                source: source.to_owned(),
            },
        );
}

fn prepare_source(executor: &Executor, workspace: &str, target: &ResolvedTarget) -> Result<String> {
    match target.provider {
        Provider::Local => Ok(target.source.clone()),
        Provider::Ssh { .. } if is_git_source(&target.source) => {
            let home = executor.run("pwd", &[])?;
            let parent = join_path(&home, ".local/share/sshpod/workspaces");
            let destination = format!(
                "{parent}/{}-{}",
                sanitize(workspace),
                sanitize(&target.provider_name)
            );
            let probe = executor.run_status(
                "git",
                &[
                    "-C".to_owned(),
                    destination.clone(),
                    "rev-parse".to_owned(),
                    "--is-inside-work-tree".to_owned(),
                ],
            )?;
            if !probe.status.success() {
                executor.run("mkdir", &["-p".to_owned(), parent.clone()])?;
                executor.run(
                    "git",
                    &[
                        "clone".to_owned(),
                        "--".to_owned(),
                        target.source.clone(),
                        destination.clone(),
                    ],
                )?;
            }
            Ok(destination)
        }
        Provider::Ssh { .. } => {
            if Path::new(&target.source).exists() {
                bail!(
                    "local source cannot currently be used with SSH provider; local source synchronization to SSH providers is not implemented yet"
                );
            }
            Ok(target.source.clone())
        }
    }
}

fn load_project(executor: &Executor, source: &str) -> Result<Project> {
    match executor.provider() {
        Provider::Local => Ok(Project::Local(devcontainer::discover_local(Path::new(
            source,
        ))?)),
        Provider::Ssh { .. } => load_remote_project(executor, source),
    }
}

fn load_remote_project(executor: &Executor, source: &str) -> Result<Project> {
    for relative in devcontainer::config_relative_paths() {
        let path = join_path(source, relative);
        let probe = executor.run_status("test", &["-f".to_owned(), path.clone()])?;
        if probe.status.success() {
            let contents = executor.run("cat", std::slice::from_ref(&path))?;
            let config = DevContainer::parse(contents.as_bytes(), &path)?;
            let config_directory = Path::new(&path)
                .parent()
                .context("remote devcontainer path has no parent")?
                .display()
                .to_string();
            return Ok(Project::Remote {
                root: source.to_owned(),
                config_directory,
                config,
            });
        }
        if probe.status.code() != Some(1) {
            bail!(
                "failed to inspect devcontainer config on provider {:?} ({}): {}",
                executor.provider_name(),
                probe.status,
                probe.stderr.trim()
            );
        }
    }
    bail!(
        ".devcontainer/devcontainer.json not found in remote workspace source {source:?} (also checked .devcontainer.json)"
    )
}

fn start_project(
    executor: &Executor,
    workspace: &str,
    provider: &str,
    project: &Project,
) -> Result<UpResult> {
    let config = project.config();
    let source = project.root();
    let workspace_folder = substituted_workspace_folder(config, &source, workspace)?;
    let container = container_name(workspace, provider);
    let mounts = resolved_mounts(config, &source, &workspace_folder, workspace)?;
    let container_environment = resolved_container_environment(
        &config.container_env,
        &source,
        &workspace_folder,
        workspace,
    )?;
    let remote_environment =
        resolved_remote_environment(&config.remote_env, &source, &workspace_folder, workspace)?;

    lifecycle::run_host(executor, &source, config.initialize_command.as_ref())?;
    let state = podman::container_status(executor, &container)?;
    let created = state == ContainerState::Missing;
    if created {
        let image = resolve_image(executor, workspace, provider, project)?;
        podman::create(
            executor,
            &ContainerSpec {
                name: &container,
                workspace,
                provider,
                image: &image,
                mounts: &mounts,
                environment: &container_environment,
                user: config.container_user.as_deref(),
            },
        )?;
        podman::start(executor, &container)?;
        for (name, command) in [
            ("onCreateCommand", config.on_create_command.as_ref()),
            (
                "updateContentCommand",
                config.update_content_command.as_ref(),
            ),
            ("postCreateCommand", config.post_create_command.as_ref()),
        ] {
            lifecycle::run_container(
                executor,
                &container,
                &workspace_folder,
                config.remote_user.as_deref(),
                &remote_environment,
                name,
                command,
            )?;
        }
    } else if state == ContainerState::Stopped {
        podman::start(executor, &container)?;
    }
    lifecycle::run_container(
        executor,
        &container,
        &workspace_folder,
        config.remote_user.as_deref(),
        &remote_environment,
        "postStartCommand",
        config.post_start_command.as_ref(),
    )?;
    Ok(UpResult {
        workspace: workspace.to_owned(),
        provider: provider.to_owned(),
        container,
        created,
    })
}

fn resolve_image(
    executor: &Executor,
    workspace: &str,
    provider: &str,
    project: &Project,
) -> Result<String> {
    let config = project.config();
    if let Some(image) = &config.image {
        return Ok(image.clone());
    }
    let build = config
        .build
        .as_ref()
        .context("validated build configuration is missing")?;
    let directory = project.config_directory();
    let dockerfile = join_path(&directory, &build.dockerfile);
    let context = join_path(&directory, &build.context);
    let tag = image_name(workspace, provider);
    podman::build_image(executor, &tag, &dockerfile, &context)?;
    Ok(tag)
}

fn substituted_workspace_folder(
    config: &DevContainer,
    source: &str,
    workspace: &str,
) -> Result<String> {
    let default = format!("/workspaces/{}", sanitize(workspace));
    let value = config.workspace_folder.as_deref().unwrap_or(&default);
    devcontainer::substitute(value, source, &default, workspace)
}

fn resolved_mounts(
    config: &DevContainer,
    source: &str,
    workspace_folder: &str,
    workspace: &str,
) -> Result<Vec<String>> {
    ensure!(
        !source.contains(','),
        "workspace source paths containing commas are not supported yet"
    );
    let workspace_mount = if let Some(mount) = &config.workspace_mount {
        devcontainer::substitute(mount, source, workspace_folder, workspace)?
    } else {
        format!("type=bind,source={source},target={workspace_folder}")
    };
    let mut mounts = vec![workspace_mount];
    for mount in &config.mounts {
        let mount = mount.as_str().context("validated mount is not a string")?;
        mounts.push(devcontainer::substitute(
            mount,
            source,
            workspace_folder,
            workspace,
        )?);
    }
    Ok(mounts)
}

fn resolved_container_environment(
    environment: &BTreeMap<String, String>,
    source: &str,
    workspace_folder: &str,
    workspace: &str,
) -> Result<BTreeMap<String, String>> {
    environment
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                devcontainer::substitute(value, source, workspace_folder, workspace)?,
            ))
        })
        .collect()
}

fn resolved_remote_environment(
    environment: &BTreeMap<String, Option<String>>,
    source: &str,
    workspace_folder: &str,
    workspace: &str,
) -> Result<BTreeMap<String, Option<String>>> {
    environment
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                value
                    .as_deref()
                    .map(|value| {
                        devcontainer::substitute(value, source, workspace_folder, workspace)
                    })
                    .transpose()?,
            ))
        })
        .collect()
}

fn observe_target(config: &Config, workspace: &str, provider_name: &str) -> ObservedState {
    let Some(provider) = config.providers.get(provider_name) else {
        return ObservedState::Error;
    };
    let executor = Executor::new(provider_name, provider);
    if let Err(error) = podman::check_available(&executor) {
        return if error.to_string().contains("unreachable") {
            ObservedState::Unreachable
        } else {
            ObservedState::Error
        };
    }
    match podman::container_status(&executor, &container_name(workspace, provider_name)) {
        Ok(ContainerState::Running) => ObservedState::Running,
        Ok(ContainerState::Stopped) => ObservedState::Stopped,
        Ok(ContainerState::Missing) => ObservedState::Missing,
        Err(error) if error.to_string().contains("unreachable") => ObservedState::Unreachable,
        Err(_) => ObservedState::Error,
    }
}

pub(crate) fn container_name(workspace: &str, provider: &str) -> String {
    format!("sshpod-{}-{}", sanitize(workspace), sanitize(provider))
}

fn image_name(workspace: &str, provider: &str) -> String {
    format!("localhost/{}:latest", container_name(workspace, provider))
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn join_path(base: &str, relative: &str) -> String {
    let relative = Path::new(relative);
    if relative.is_absolute() {
        relative.display().to_string()
    } else {
        PathBuf::from(base).join(relative).display().to_string()
    }
}

fn is_git_source(source: &str) -> bool {
    source.contains("://")
        || source.starts_with("git@")
        || Path::new(source)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("git"))
}

#[cfg(test)]
mod tests {
    use super::{container_name, is_git_source};

    #[test]
    fn deterministic_container_names_include_provider() {
        assert_eq!(
            container_name("My.Workspace", "sand_box"),
            "sshpod-my-workspace-sand_box"
        );
        assert_eq!(
            container_name("My.Workspace", "sand_box"),
            container_name("My.Workspace", "sand_box")
        );
    }

    #[test]
    fn recognizes_remote_git_sources() {
        assert!(is_git_source("git@github.com:owner/repository.git"));
        assert!(is_git_source("https://github.com/owner/repository"));
        assert!(!is_git_source(".local/share/project"));
    }
}
