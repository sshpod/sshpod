use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

use crate::{
    provider::{CommandOutput, Executor, Provider},
    workspace::ContainerState,
};

pub(crate) struct ContainerSpec<'a> {
    pub(crate) name: &'a str,
    pub(crate) workspace: &'a str,
    pub(crate) provider: &'a str,
    pub(crate) image: &'a str,
    pub(crate) mounts: &'a [String],
    pub(crate) environment: &'a BTreeMap<String, String>,
    pub(crate) user: Option<&'a str>,
}

pub(crate) fn check_available(executor: &Executor) -> Result<String> {
    let output = executor
        .run_status("podman", &["--version".to_owned()])
        .with_context(|| {
            format!(
                "Podman is not installed on provider {:?} or cannot be executed",
                executor.provider_name()
            )
        })?;
    availability_output(executor, &output)
}

fn availability_output(executor: &Executor, output: &CommandOutput) -> Result<String> {
    if output.status.success() {
        let version = output.stdout.trim();
        if version.is_empty() {
            bail!(
                "Podman returned an empty version on provider {:?}",
                executor.provider_name()
            );
        }
        return Ok(version.to_owned());
    }
    if matches!(executor.provider(), Provider::Ssh { .. }) && output.status.code() == Some(255) {
        bail!(
            "provider {:?} is unreachable: {}",
            executor.provider_name(),
            diagnostic(output)
        );
    }
    if matches!(executor.provider(), Provider::Ssh { .. })
        && (output.status.code() == Some(127)
            || output
                .stderr
                .to_ascii_lowercase()
                .contains("podman: not found")
            || output
                .stderr
                .to_ascii_lowercase()
                .contains("podman: command not found"))
    {
        bail!(
            "Podman is not installed on provider {:?}: {}",
            executor.provider_name(),
            diagnostic(output)
        );
    }
    bail!(
        "Podman command failed on provider {:?} ({}): {}",
        executor.provider_name(),
        output.status,
        diagnostic(output)
    )
}

pub(crate) fn container_exists(executor: &Executor, name: &str) -> Result<bool> {
    let output = executor.run_status(
        "podman",
        &["container".to_owned(), "exists".to_owned(), name.to_owned()],
    )?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => command_failure(executor, "podman container exists", &output),
    }
}

pub(crate) fn container_status(executor: &Executor, name: &str) -> Result<ContainerState> {
    if !container_exists(executor, name)? {
        return Ok(ContainerState::Missing);
    }
    let status = executor.run(
        "podman",
        &[
            "container".to_owned(),
            "inspect".to_owned(),
            "--format".to_owned(),
            "{{.State.Status}}".to_owned(),
            name.to_owned(),
        ],
    )?;
    if status == "running" {
        Ok(ContainerState::Running)
    } else {
        Ok(ContainerState::Stopped)
    }
}

pub(crate) fn build_image(
    executor: &Executor,
    tag: &str,
    dockerfile: &str,
    context: &str,
) -> Result<()> {
    executor.run(
        "podman",
        &[
            "build".to_owned(),
            "--file".to_owned(),
            dockerfile.to_owned(),
            "--tag".to_owned(),
            tag.to_owned(),
            context.to_owned(),
        ],
    )?;
    Ok(())
}

pub(crate) fn create(executor: &Executor, spec: &ContainerSpec<'_>) -> Result<()> {
    executor.run("podman", &create_args(spec))?;
    Ok(())
}

pub(crate) fn start(executor: &Executor, name: &str) -> Result<()> {
    executor.run("podman", &["start".to_owned(), name.to_owned()])?;
    Ok(())
}

pub(crate) fn stop(executor: &Executor, name: &str) -> Result<()> {
    executor.run("podman", &["stop".to_owned(), name.to_owned()])?;
    Ok(())
}

pub(crate) fn exec(
    executor: &Executor,
    container: &str,
    working_directory: &str,
    user: Option<&str>,
    environment: &BTreeMap<String, Option<String>>,
    program: &str,
    command_args: &[String],
) -> Result<()> {
    let args = exec_args(
        container,
        working_directory,
        user,
        environment,
        program,
        command_args,
    );
    executor.run("podman", &args)?;
    Ok(())
}

fn create_args(spec: &ContainerSpec<'_>) -> Vec<String> {
    let mut args = vec![
        "create".to_owned(),
        "--name".to_owned(),
        spec.name.to_owned(),
        "--label".to_owned(),
        "sshpod.managed=true".to_owned(),
        "--label".to_owned(),
        format!("sshpod.workspace={}", spec.workspace),
        "--label".to_owned(),
        format!("sshpod.provider={}", spec.provider),
    ];
    for (key, value) in spec.environment {
        args.extend(["--env".to_owned(), format!("{key}={value}")]);
    }
    if let Some(user) = spec.user {
        args.extend(["--user".to_owned(), user.to_owned()]);
    }
    for mount in spec.mounts {
        args.extend(["--mount".to_owned(), mount.clone()]);
    }
    args.extend([
        "--entrypoint".to_owned(),
        "/bin/sh".to_owned(),
        spec.image.to_owned(),
        "-c".to_owned(),
        "while sleep 1000; do :; done".to_owned(),
    ]);
    args
}

fn exec_args(
    container: &str,
    working_directory: &str,
    user: Option<&str>,
    environment: &BTreeMap<String, Option<String>>,
    program: &str,
    command_args: &[String],
) -> Vec<String> {
    let mut args = vec![
        "exec".to_owned(),
        "--workdir".to_owned(),
        working_directory.to_owned(),
    ];
    if let Some(user) = user {
        args.extend(["--user".to_owned(), user.to_owned()]);
    }
    for (key, value) in environment {
        if let Some(value) = value {
            args.extend(["--env".to_owned(), format!("{key}={value}")]);
        }
    }
    args.push(container.to_owned());
    args.push(program.to_owned());
    args.extend_from_slice(command_args);
    args
}

fn command_failure<T>(executor: &Executor, operation: &str, output: &CommandOutput) -> Result<T> {
    if matches!(executor.provider(), Provider::Ssh { .. }) && output.status.code() == Some(255) {
        bail!(
            "provider {:?} is unreachable: {}",
            executor.provider_name(),
            diagnostic(output)
        );
    }
    bail!(
        "{operation} failed on provider {:?} ({}): {}",
        executor.provider_name(),
        output.status,
        diagnostic(output)
    )
}

fn diagnostic(output: &CommandOutput) -> &str {
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        "no diagnostic output"
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    use super::{ContainerSpec, availability_output, create_args, exec_args};
    use crate::provider::{CommandOutput, Executor, Provider};

    fn output(code: i32, stderr: &str) -> CommandOutput {
        CommandOutput {
            status: ExitStatus::from_raw(code << 8),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn constructs_create_command_with_labels_mounts_and_environment() {
        let environment = BTreeMap::from([("MODE".to_owned(), "dev".to_owned())]);
        let mounts = vec!["type=bind,source=/source,target=/workspaces/demo".to_owned()];
        let args = create_args(&ContainerSpec {
            name: "sshpod-demo-local",
            workspace: "demo",
            provider: "local",
            image: "alpine",
            mounts: &mounts,
            environment: &environment,
            user: Some("developer"),
        });
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--name", "sshpod-demo-local"])
        );
        assert!(args.contains(&"sshpod.managed=true".to_owned()));
        assert!(args.contains(&"sshpod.workspace=demo".to_owned()));
        assert!(args.contains(&"sshpod.provider=local".to_owned()));
        assert!(args.contains(&"MODE=dev".to_owned()));
        assert!(args.contains(&"type=bind,source=/source,target=/workspaces/demo".to_owned()));
    }

    #[test]
    fn constructs_exec_command_with_remote_environment() {
        let environment = BTreeMap::from([("EDITOR".to_owned(), Some("vi".to_owned()))]);
        let args = exec_args(
            "sshpod-demo-local",
            "/workspaces/demo",
            Some("developer"),
            &environment,
            "sh",
            &["-c".to_owned(), "true".to_owned()],
        );
        assert!(args.contains(&"EDITOR=vi".to_owned()));
        assert!(args.windows(2).any(|pair| pair == ["--user", "developer"]));
    }

    #[test]
    fn distinguishes_remote_availability_failures() {
        let executor = Executor::new(
            "sandbox",
            &Provider::Ssh {
                host: "sandbox".to_owned(),
                podman: "podman".to_owned(),
                ssh_args: Vec::new(),
            },
        );
        let unreachable = availability_output(&executor, &output(255, "connection refused"));
        assert!(
            unreachable
                .err()
                .is_some_and(|error| error.to_string().contains("unreachable"))
        );
        let missing = availability_output(&executor, &output(127, "podman: not found"));
        assert!(
            missing
                .err()
                .is_some_and(|error| error.to_string().contains("not installed"))
        );
    }
}
