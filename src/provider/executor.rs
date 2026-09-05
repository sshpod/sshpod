use std::{
    path::Path,
    process::{Command, ExitStatus, Stdio},
};

use anyhow::{Context, Result, bail};

use super::Provider;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CommandSpec {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct CommandOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Executor {
    name: String,
    provider: Provider,
}

impl Executor {
    pub(crate) fn new(name: &str, provider: &Provider) -> Self {
        Self {
            name: name.to_owned(),
            provider: provider.clone(),
        }
    }

    pub(crate) fn provider_name(&self) -> &str {
        &self.name
    }

    pub(crate) fn provider(&self) -> &Provider {
        &self.provider
    }

    pub(crate) fn command_spec(&self, program: &str, args: &[String]) -> CommandSpec {
        match &self.provider {
            Provider::Local { .. } => CommandSpec {
                program: program.to_owned(),
                args: args.to_vec(),
            },
            Provider::Ssh { host, .. } => CommandSpec {
                program: "ssh".to_owned(),
                args: vec!["--".to_owned(), host.clone(), shell_join(program, args)],
            },
        }
    }

    pub(crate) fn run_status(&self, program: &str, args: &[String]) -> Result<CommandOutput> {
        self.run_status_in(None, program, args)
    }

    pub(crate) fn run_status_in(
        &self,
        directory: Option<&str>,
        program: &str,
        args: &[String],
    ) -> Result<CommandOutput> {
        let spec = if let Some(directory) = directory {
            self.command_spec_in(directory, program, args)
        } else {
            self.command_spec(program, args)
        };
        let mut command = Command::new(&spec.program);
        command.args(&spec.args).stdin(Stdio::null());
        if matches!(self.provider, Provider::Local { .. })
            && let Some(directory) = directory
        {
            command.current_dir(Path::new(directory));
        }
        let output = command.output().with_context(|| match self.provider {
            Provider::Local { .. } => {
                format!("could not execute {program:?} on provider {:?}", self.name)
            }
            Provider::Ssh { .. } => {
                format!("could not execute system SSH for provider {:?}", self.name)
            }
        })?;
        Ok(CommandOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    pub(crate) fn run(&self, program: &str, args: &[String]) -> Result<String> {
        self.run_in(None, program, args)
    }

    pub(crate) fn run_in(
        &self,
        directory: Option<&str>,
        program: &str,
        args: &[String],
    ) -> Result<String> {
        let output = self.run_status_in(directory, program, args)?;
        if !output.status.success() {
            if matches!(self.provider, Provider::Ssh { .. }) && output.status.code() == Some(255) {
                bail!(
                    "provider {:?} is unreachable: {}",
                    self.name,
                    diagnostic(&output)
                );
            }
            bail!(
                "{program} failed on provider {:?} ({}): {}",
                self.name,
                output.status,
                diagnostic(&output)
            );
        }
        Ok(output.stdout.trim().to_owned())
    }

    fn command_spec_in(&self, directory: &str, program: &str, args: &[String]) -> CommandSpec {
        match &self.provider {
            Provider::Local { .. } => self.command_spec(program, args),
            Provider::Ssh { host, .. } => CommandSpec {
                program: "ssh".to_owned(),
                args: vec![
                    "--".to_owned(),
                    host.clone(),
                    format!(
                        "cd {} && {}",
                        shell_quote(directory),
                        shell_join(program, args)
                    ),
                ],
            },
        }
    }
}

fn diagnostic(output: &CommandOutput) -> &str {
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        "no diagnostic output"
    } else {
        stderr
    }
}

fn shell_join(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b'.' | b'_' | b'-' | b':' | b'@' | b'=' | b',' | b'{' | b'}'
                )
        })
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Executor, shell_quote};
    use crate::provider::Provider;

    #[test]
    fn constructs_local_command() {
        let executor = Executor::new(
            "local",
            &Provider::Local {
                podman: "podman".into(),
            },
        );
        let spec = executor.command_spec("podman", &["--version".to_owned()]);
        assert_eq!(spec.program, "podman");
        assert_eq!(spec.args, ["--version"]);
    }

    #[test]
    fn constructs_quoted_ssh_command() {
        let executor = Executor::new(
            "sandbox",
            &Provider::Ssh {
                host: "sandbox".to_owned(),
                podman: "podman".to_owned(),
                ssh_args: Vec::new(),
            },
        );
        let spec = executor.command_spec(
            "podman",
            &["create".to_owned(), "name with space".to_owned()],
        );
        assert_eq!(spec.program, "ssh");
        assert_eq!(
            spec.args,
            ["--", "sandbox", "podman create 'name with space'"]
        );
        assert_eq!(shell_quote("a'b"), "'a'\"'\"'b'");
    }
}
