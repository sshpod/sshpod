use anyhow::Result;
use sshpod::cli;

fn main() -> Result<()> {
    let action = cli::start()?;

    match action {
        cli::actions::Action::List => cli::actions::list::execute(),
        cli::actions::Action::Up {
            workspace,
            provider,
            devcontainer,
        } => cli::actions::up::execute(&workspace, provider.as_deref(), devcontainer.as_deref()),
        cli::actions::Action::Down {
            workspace,
            provider,
        } => cli::actions::down::execute(&workspace, provider.as_deref()),
        cli::actions::Action::ProviderList => cli::actions::provider::list(),
        cli::actions::Action::ProviderAdd {
            name,
            provider_type,
            host,
            podman,
            ssh_args,
        } => cli::actions::provider::add(
            &name,
            &provider_type,
            host.as_deref(),
            podman.as_deref(),
            &ssh_args,
        ),
        cli::actions::Action::ProviderDelete { name } => cli::actions::provider::delete(&name),
        cli::actions::Action::Doctor => cli::actions::doctor::execute(),
    }
}
