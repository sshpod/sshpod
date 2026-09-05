mod actions;
mod args;

use clap::Parser;

pub fn run() -> anyhow::Result<()> {
    match args::Args::parse().command {
        args::Command::Doctor => actions::doctor::execute(),
    }
}
