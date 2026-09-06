# Changelog

## 0.1.0 (unreleased)

- Local and OpenSSH-backed Podman providers with persistent YAML/XDG configuration.
- Typed `devcontainer.json` discovery, strict JSONC parsing, schema-aware
  validation, normalization, forward-compatible diagnostics, and image,
  Dockerfile, and Compose source models.
- Offline conformance tests against a pinned official base schema, including
  union, enum, boundary, invalid-type, fixture, and documented-divergence cases.
- Specification-order Dev Container discovery with selectable nested configurations.
- Workspace `up`, `down`, and `list` commands with deterministic containers and labels.
- Basic bind mounts, environment, users, variable substitution, and lifecycle commands.
- Local Podman executable check through `sshpod doctor`.
- Strict linting, tests, dependency checks, and package preparation.
