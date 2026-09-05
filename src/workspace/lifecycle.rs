use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::{devcontainer::LifecycleCommand, podman, provider::Executor};

#[cfg(test)]
use crate::devcontainer::DevContainer;

pub(crate) fn run_host(
    executor: &Executor,
    workspace_directory: &str,
    command: Option<&LifecycleCommand>,
) -> Result<()> {
    if let Some(command) = command {
        let (program, args) = command_parts(command)?;
        executor
            .run_in(Some(workspace_directory), &program, &args)
            .context("initializeCommand failed")?;
    }
    Ok(())
}

pub(crate) fn run_container(
    executor: &Executor,
    container: &str,
    workspace_folder: &str,
    user: Option<&str>,
    environment: &BTreeMap<String, Option<String>>,
    name: &str,
    command: Option<&LifecycleCommand>,
) -> Result<()> {
    if let Some(command) = command {
        let (program, args) = command_parts(command)?;
        podman::exec(
            executor,
            container,
            workspace_folder,
            user,
            environment,
            &program,
            &args,
        )
        .with_context(|| format!("{name} failed"))?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn command_order(config: &DevContainer, new_container: bool) -> Vec<&'static str> {
    let mut order = Vec::new();
    if config.initialize_command.is_some() {
        order.push("initializeCommand");
    }
    if new_container {
        for (name, command) in [
            ("onCreateCommand", config.on_create_command.as_ref()),
            (
                "updateContentCommand",
                config.update_content_command.as_ref(),
            ),
            ("postCreateCommand", config.post_create_command.as_ref()),
        ] {
            if command.is_some() {
                order.push(name);
            }
        }
    }
    if config.post_start_command.is_some() {
        order.push("postStartCommand");
    }
    order
}

fn command_parts(command: &LifecycleCommand) -> Result<(String, Vec<String>)> {
    match command {
        LifecycleCommand::Shell(command) => {
            Ok(("/bin/sh".to_owned(), vec!["-c".to_owned(), command.clone()]))
        }
        LifecycleCommand::Direct(command) => {
            let (program, args) = command
                .split_first()
                .context("lifecycle command array is empty")?;
            Ok((program.clone(), args.to_vec()))
        }
        LifecycleCommand::Parallel(_) => {
            anyhow::bail!("parallel lifecycle commands are not supported yet")
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::devcontainer::DevContainer;

    use super::command_order;

    #[test]
    fn lifecycle_order_distinguishes_create_and_start() -> anyhow::Result<()> {
        let config = DevContainer::parse(
            br#"{
                "image":"alpine",
                "initializeCommand":"true",
                "onCreateCommand":"true",
                "updateContentCommand":["true"],
                "postCreateCommand":"true",
                "postStartCommand":"true"
            }"#,
            "test",
        )?;
        assert_eq!(
            command_order(&config, true),
            [
                "initializeCommand",
                "onCreateCommand",
                "updateContentCommand",
                "postCreateCommand",
                "postStartCommand"
            ]
        );
        assert_eq!(
            command_order(&config, false),
            ["initializeCommand", "postStartCommand"]
        );
        Ok(())
    }
}
