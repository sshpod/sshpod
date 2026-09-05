use anyhow::{Context, Result, bail};
use clap::ArgMatches;

use crate::cli::actions::Action;

/// Convert parsed command-line arguments into an action.
///
/// # Errors
///
/// Returns an error if the parsed arguments contain no supported subcommand.
pub fn handler(matches: &ArgMatches) -> Result<Action> {
    match matches.subcommand() {
        None | Some(("list", _)) => Ok(Action::List),
        Some(("up", arguments)) => workspace_action(arguments, true),
        Some(("down", arguments)) => workspace_action(arguments, false),
        Some(("provider", arguments)) => provider_action(arguments),
        Some(("doctor", _)) => Ok(Action::Doctor),
        Some((name, _)) => bail!("unsupported command: {name}"),
    }
}

fn workspace_action(matches: &ArgMatches, up: bool) -> Result<Action> {
    let workspace = matches
        .get_one::<String>("workspace")
        .context("workspace argument is missing")?
        .clone();
    let provider = matches.get_one::<String>("provider").cloned();
    if up {
        Ok(Action::Up {
            workspace,
            provider,
        })
    } else {
        Ok(Action::Down {
            workspace,
            provider,
        })
    }
}

fn provider_action(matches: &ArgMatches) -> Result<Action> {
    match matches.subcommand() {
        Some(("list", _)) => Ok(Action::ProviderList),
        Some(("add", arguments)) => Ok(Action::ProviderAdd {
            name: required_string(arguments, "name")?,
            provider_type: required_string(arguments, "type")?,
            host: arguments.get_one::<String>("host").cloned(),
        }),
        Some(("delete", arguments)) => Ok(Action::ProviderDelete {
            name: required_string(arguments, "name")?,
        }),
        Some((name, _)) => bail!("unsupported provider command: {name}"),
        None => bail!("no provider command selected"),
    }
}

fn required_string(matches: &ArgMatches, name: &str) -> Result<String> {
    matches
        .get_one::<String>(name)
        .cloned()
        .with_context(|| format!("{name} argument is missing"))
}

#[cfg(test)]
mod tests {
    use super::handler;
    use crate::cli::{actions::Action, commands};

    #[test]
    fn dispatches_commands() -> anyhow::Result<()> {
        let matches = commands::new().try_get_matches_from(["sshpod", "doctor"])?;
        assert_eq!(handler(&matches)?, Action::Doctor);
        let matches = commands::new().try_get_matches_from([
            "sshpod",
            "up",
            "demo",
            "--provider",
            "sandbox",
        ])?;
        assert_eq!(
            handler(&matches)?,
            Action::Up {
                workspace: "demo".to_owned(),
                provider: Some("sandbox".to_owned())
            }
        );
        Ok(())
    }

    #[test]
    fn root_dispatches_list() -> anyhow::Result<()> {
        let matches = commands::new().try_get_matches_from(["sshpod"])?;
        assert_eq!(handler(&matches)?, Action::List);
        Ok(())
    }
}
