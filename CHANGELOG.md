# Changelog

sshpod is experimental. APIs, configuration, and behavior may change without
compatibility guarantees before 1.0. Entries under `Unreleased` are curated and
may be combined or rewritten as the prototype evolves.

## Unreleased

### Added

- Local and OpenSSH-backed Podman providers with persistent YAML/XDG
  configuration.
- Workspace `up`, `down`, and `list` commands with deterministic container names
  and labels.
- Specification-order Dev Container discovery with explicit selection when a
  workspace contains multiple configurations.
- JSONC parsing, validation, and normalization for image, Dockerfile, and Compose
  Dev Container configurations, with actionable diagnostics.
- Runtime support for image and simple Dockerfile sources, string-form mounts,
  environment variables, users, workspace paths, and basic lifecycle commands.
- Podman availability checks through `sshpod doctor`.

### Known limitations

- Compose execution, Features, port forwarding, advanced build options,
  object-form mounts, and several advanced lifecycle and user behaviors are
  parsed but not yet executed by the runtime.
