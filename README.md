# sshpod

sshpod is an early-stage CLI being built to provide persistent Podman development
workspaces locally or on remote Linux machines reachable through SSH.

Version 0.1.x currently checks the local Podman executable. Workspace creation and
lifecycle management are not implemented yet.

## Usage

Build and install from this checkout with Rust 1.88 or later:

```sh
cargo install --path . --locked
sshpod --help
sshpod -V
sshpod --version
sshpod doctor
```

`doctor` executes `podman --version`, reports its version text, and exits nonzero
with an actionable error if Podman cannot run. Install Podman and make it available
on `PATH`. This check does not start containers or validate runtime health, SSH,
or remote connectivity. Help and version do not require Podman.

Help uses terminal-aware colors. `-V` prints the package version; `--version`
also prints the build's Git commit hash (`unknown` when Git metadata is unavailable,
including builds from packaged source). No timestamp is embedded.

## Intended model

```text
sshpod -> Podman -> workspace
sshpod -> Podman remote/SSH -> remote Podman -> workspace
```

Remote Linux hosts must already exist and be reachable over SSH. Future remote
operations will use Podman's existing `podman system connection` configuration.
Podman supplies the runtime and SSH transport.

The long-term use case is persistent compute for humans and coding agents such as
Codex, Claude Code, and OpenCode, allowing work to continue while the user's laptop
is offline. External tools such as Herdr, Moch, and tmux manage agents and sessions.

sshpod manages workspace/container lifecycle. It is not intended to provision VMs,
replace Podman or SSH, replace Herdr/tmux, orchestrate AI agents, or become a generic
Kubernetes/Docker/cloud-provider abstraction.

## Roadmap

- Workspace lifecycle with local and remote Podman connections
- Persistent volumes
- Repository/worktree handling
- Resource limits
- Integration with external agent/session managers

These are future concepts, not available commands or promised APIs.

## Development

CLI parsing lives in `src/cli/args.rs`, presentation in `src/cli/actions/`, and
Podman process interaction in `src/podman/`. Commands use `std::process::Command`
without a shell. There is no async runtime or provider layer.

Run `just test` for formatting, checking, tests, and strict Clippy. Ordinary tests
do not need Podman. `just ci` also requires `cargo-deny` for advisory, license, and
dependency-source checks. CI checks stable Rust and the minimum Rust version.

Run `just release-check` on a clean, committed tree to validate packaging and run
a publish dry run without uploading anything.

This project uses the BSD-3-Clause license and adapts engineering conventions from
[cron-when](https://github.com/nbari/cron-when).
