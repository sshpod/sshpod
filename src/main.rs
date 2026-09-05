mod cli;
mod podman;

fn main() -> anyhow::Result<()> {
    cli::run()
}
