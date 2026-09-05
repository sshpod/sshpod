use clap::{
    Arg, ArgAction, ColorChoice, Command,
    builder::styling::{AnsiColor, Effects, Styles},
};

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Blue.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Green.on_default())
}

/// Build the sshpod command-line interface.
#[must_use]
pub fn new() -> Command {
    Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .long_version(concat!(
            env!("CARGO_PKG_VERSION"),
            " - ",
            env!("SSHPOD_GIT_COMMIT")
        ))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .long_about("Start and stop development containers from a project's devcontainer.json using local Podman or Podman on an existing SSH host.")
        .color(ColorChoice::Auto)
        .styles(styles())
        .subcommand(Command::new("list").about("List configured workspaces and their status"))
        .subcommand(workspace_command("up", "Create or start a workspace", true))
        .subcommand(workspace_command(
            "down",
            "Stop a workspace without deleting it",
            false,
        ))
        .subcommand(provider_command())
        .subcommand(Command::new("doctor").about(
            "Check that the local Podman CLI can run and report its version",
        ))
}

fn workspace_command(name: &'static str, about: &'static str, accepts_config: bool) -> Command {
    let command = Command::new(name)
        .about(about)
        .arg(
            Arg::new("workspace")
                .value_name("WORKSPACE")
                .help("Logical workspace name")
                .required(true),
        )
        .arg(
            Arg::new("provider")
                .long("provider")
                .value_name("NAME")
                .help("Provider target to use"),
        );
    if accepts_config {
        command.arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("Select a discovered devcontainer.json by workspace-relative path"),
        )
    } else {
        command
    }
}

fn provider_command() -> Command {
    Command::new("provider")
        .about("Manage local and SSH providers")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("list").about("List configured providers"))
        .subcommand(
            Command::new("add")
                .about("Add a local or SSH provider")
                .arg(Arg::new("name").value_name("NAME").required(true))
                .arg(
                    Arg::new("type")
                        .long("type")
                        .value_name("local|ssh")
                        .value_parser(["local", "ssh"])
                        .required(true),
                )
                .arg(
                    Arg::new("host")
                        .long("host")
                        .value_name("SSH_CONFIG_HOST")
                        .help("OpenSSH host or alias; required for an SSH provider"),
                )
                .arg(
                    Arg::new("podman")
                        .long("podman")
                        .value_name("COMMAND")
                        .help("Podman-compatible executable or path (default: podman)"),
                )
                .arg(
                    Arg::new("ssh-arg")
                        .long("ssh-arg")
                        .value_name("ARG")
                        .help("OpenSSH argument to store; may be repeated")
                        .action(ArgAction::Append)
                        .allow_hyphen_values(true),
                ),
        )
        .subcommand(
            Command::new("delete")
                .about("Delete a provider and its workspace target bindings")
                .arg(Arg::new("name").value_name("NAME").required(true)),
        )
}

#[cfg(test)]
mod tests {
    use super::new;

    #[test]
    fn command_definition_is_valid() {
        new().debug_assert();
    }

    #[test]
    fn help_uses_reference_palette() {
        let help = new().render_help().ansi().to_string();
        for style in [
            super::styles().get_header(),
            super::styles().get_usage(),
            super::styles().get_literal(),
        ] {
            assert!(help.contains(&style.render().to_string()));
        }
    }

    #[test]
    fn parses_doctor() -> anyhow::Result<()> {
        let matches = new().try_get_matches_from(["sshpod", "doctor"])?;
        assert_eq!(matches.subcommand_name(), Some("doctor"));
        Ok(())
    }

    #[test]
    fn parses_workspace_and_provider_commands() -> anyhow::Result<()> {
        let up = new().try_get_matches_from([
            "sshpod",
            "up",
            "demo",
            "--provider",
            "sandbox",
            "--config",
            ".devcontainer/rust/devcontainer.json",
        ])?;
        assert_eq!(up.subcommand_name(), Some("up"));
        assert!(
            new()
                .try_get_matches_from(["sshpod", "down", "demo", "--config", ".devcontainer.json"])
                .is_err()
        );
        let provider = new().try_get_matches_from([
            "sshpod",
            "provider",
            "add",
            "sandbox",
            "--type",
            "ssh",
            "--host",
            "sandbox",
            "--podman",
            "/usr/bin/podman",
            "--ssh-arg=-A",
        ])?;
        assert_eq!(provider.subcommand_name(), Some("provider"));
        Ok(())
    }

    #[test]
    fn rejects_unknown_commands_and_allows_root_list() {
        assert!(new().try_get_matches_from(["sshpod", "create"]).is_err());
        assert!(new().try_get_matches_from(["sshpod"]).is_ok());
    }
}
