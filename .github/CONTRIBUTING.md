# Contributing

Start with a focused issue or pull request describing the problem and expected
behavior. For vulnerabilities, follow [SECURITY.md](SECURITY.md).

sshpod manages Podman workspace/container lifecycle on existing hosts. Keep SSH,
container execution, and agent/session management in their respective tools.
Discuss changes that expand this scope before implementing them.

Use stable Rust, edition 2024, and preserve compatibility with `rust-version` in
Cargo.toml. Run `just test` and `just deny` before submitting. Tests must work
without an installed Podman unless explicitly separated as integration checks.
Add dependencies only when they solve an immediate need that std cannot reasonably
handle. Keep CLI parsing, presentation, and Podman execution separate.

Use plain, descriptive commit messages without `feat:`, `chore:`, or similar
prefixes. Do not add co-author trailers.

In pull requests, explain the behavior change, relevant validation, and remaining
limitations. Do not include secrets or generated build artifacts.
