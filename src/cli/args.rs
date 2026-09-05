use clap::{
    ColorChoice, Parser, Subcommand,
    builder::styling::{AnsiColor, Effects, Styles},
};

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Blue.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Green.on_default())
}

#[derive(Debug, Parser)]
#[command(
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), " - ", env!("SSHPOD_GIT_COMMIT")),
    author,
    color = ColorChoice::Auto,
    styles = styles(),
    about,
    long_about = "Early-stage Podman workspace CLI. Version 0.1.x checks local prerequisites; workspace lifecycle is planned."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that the local Podman CLI can run and report its version
    Doctor,
}

#[cfg(test)]
mod tests {
    use super::{Args, Command};
    use clap::{CommandFactory, Parser};

    #[test]
    fn help_uses_reference_palette() {
        let help = Args::command().render_help().ansi().to_string();
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
        Args::command().debug_assert();
        assert!(matches!(
            Args::try_parse_from(["sshpod", "doctor"])?.command,
            Command::Doctor
        ));
        Ok(())
    }

    #[test]
    fn rejects_unknown_commands_and_missing_command() {
        assert!(Args::try_parse_from(["sshpod", "create"]).is_err());
        assert!(Args::try_parse_from(["sshpod"]).is_err());
    }
}
