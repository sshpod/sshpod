# Repository Guidelines

This file is the engineering contract for sshpod. Apply it to production code,
tests, reviews, and automation unless a narrower `AGENTS.md` documents a justified
exception.

## Project Structure and Architecture

`src/bin/sshpod.rs` is the CLI entry point and `src/lib.rs` is the intentional
public library surface. CLI parsing belongs in `src/cli/`, application
configuration in `src/config/`, Dev Container processing in `src/devcontainer/`,
workspace policy in `src/workspace/`, provider transport in `src/provider/`, and
Podman command construction in `src/podman/`. Integration tests live in `tests/`,
fixtures in `tests/fixtures/`, and CI and policies in `.github/`.

Dependencies and data must follow these boundaries:

```text
devcontainer.json
  -> parse -> validate -> normalize
  -> workspace/orchestration plan
  -> provider/runtime execution
  -> SSH / Podman
```

- Dev Container parsing must not import SSH or Podman execution code or emit CLI
  arguments. Each parse, validation, normalization, planning, and execution phase
  must remain independently testable.
- The Podman layer consumes typed plans and must not understand JSON/JSONC schema
  quirks. The SSH provider transports commands and must not contain workspace
  policy. Avoid circular conceptual dependencies.
- Prefer concrete implementations. Add a trait only at a real substitution or
  testing boundary; do not create traits merely to mock every struct. Use an enum
  when runtime alternatives are closed and a trait when open polymorphism is
  justified.
- Keep visibility minimal: private by default, `pub(crate)` for crate internals,
  and `pub` only for intended library API. Re-export stable domain concepts, not
  whole implementation trees. Breaking `src/lib.rs` APIs must be deliberate.

## File and Module Organization

Prefer cohesive modules over monoliths. Split by responsibility when a file
approaches 500-800 lines or contains unrelated parsing, policy, and execution
logic. A production Rust file MUST NOT exceed approximately 1000 lines without a
strong technical reason documented in the file and pull request. This is not a
reason to fragment code into tiny files with no meaningful boundary.

Larger subsystems should use a discoverable multi-file layout, for example:

```text
src/devcontainer/
├── mod.rs
├── discovery.rs
├── parser.rs
├── model.rs
├── validation.rs
├── error.rs
└── resolver.rs
```

Use `mod.rs` when it improves a subsystem's discoverability. It should declare
children, expose the intended API, re-export important types, and contain only
small glue code. Do not hide large implementations there. Keep functions focused;
prefer early returns over deep nesting and split giant matches that combine
unrelated responsibilities.

## Rust Engineering Rules

- Use stable Rust, edition 2024, and retain the `rust-version` in `Cargo.toml`.
  Let `rustfmt` control four-space layout. Group imports from one namespace, such
  as `use std::{path::Path, process::Command};`.
- Follow Rust naming: `snake_case` files/modules/functions, `UpperCamelCase` types,
  and `SCREAMING_SNAKE_CASE` constants.
- Strict Clippy (`all` and `pedantic`) is denied. Do not add `#[allow(...)]` in
  production code. Test-only allowances must be necessary, narrowly scoped, and
  include a reason.
- Prefer explicit domain types over raw `String`, maps, or `serde_json::Value`
  when the domain is known. Use enums for mutually exclusive states and instead
  of boolean-heavy APIs. Make invalid states difficult to construct. Use
  builder-style configuration construction when it improves validity or clarity.
- Borrow with `&str` and `&Path` when ownership is unnecessary. Do not clone just
  to appease the borrow checker without understanding the ownership boundary.
  Use `Cow` or complex lifetimes only when they provide measurable or
  architectural value.
- Use iterators when they clarify intent, not when they obscure control flow.
  Avoid clever abstractions; public APIs and functions should be small and
  intentional.
- Use native `async fn` in traits; do not add or use `async-trait`. If object
  safety becomes a problem, prefer a concrete type or enum when practical.
- Safe Rust is mandatory by default (`unsafe_code` is forbidden). Any proposal to
  permit `unsafe` requires a demonstrated need, a minimal block, documented
  safety invariants, focused tests, and an explanation of rejected safe options.
  Prefer mature safe abstractions to local unsafe primitives.

## Error Handling and Reliability

Production paths must not use `unwrap`, `expect`, `panic!`, `todo!`, or
`unimplemented!`. Even for a strong invariant, prefer returning an error; a truly
unreachable exception must be explained and tested.

Use typed errors at subsystem boundaries (`DevContainerError`, `ProviderError`,
`PodmanError`, or `WorkspaceError`). The CLI may aggregate errors, but must not
collapse causes into opaque strings too early. Preserve source chains and attach
operation, provider, workspace, command, path, line, and column context when
relevant. User-facing messages must explain what failed and how to act, while
never exposing secrets. Do not silently discard failures unless the operation is
explicitly best-effort and documented.

## Performance

Correctness and maintainability come first. For this CLI, optimize meaningful
costs: startup latency, spawned processes, SSH round trips, filesystem access,
parsing passes, allocations/copies, and blocking work in async contexts.

- Avoid intermediate `String`, `Vec`, and collection creation. Do not call
  `to_string` when a borrow suffices or collect only to iterate immediately.
  Preallocate with `with_capacity` when size is cheaply known and material.
- Parse each configuration once where practical, then pass typed results through
  `parse -> validate -> normalize -> plan -> execute`. Do not re-read or
  deserialize the same file during one operation.
- Podman and SSH processes are expensive. Prefer one safe, clear invocation over
  repeated probes or round trips, but never combine commands at the expense of
  validation, security, or useful diagnostics.
- Do not micro-optimize without evidence. Add Criterion or another benchmark
  dependency only for a concrete, important question worth tracking.

## Async and Concurrency

Use async for potentially waiting SSH, subprocess, network, or independent I/O;
keep CPU-bound parsing and validation synchronous unless evidence says otherwise.
Do not introduce an async runtime merely because the project may use async code.

When operating in an async runtime, do not perform significantly blocking
filesystem, process, or network work on executor threads. Use the established
process abstraction or `tokio::process::Command` if Tokio is the selected runtime.
Never hold a lock across `.await`. Minimize shared mutable state; prefer ownership
transfer, immutable state, message passing, or local state over `Arc<Mutex<_>>`.
When synchronization is necessary, document why, choose the narrowest primitive,
and keep critical sections short. Bound task spawning, channels, and concurrency;
results should remain deterministic where practical.

## Security and Trust Boundaries

Treat CLI arguments, Dev Container files, repositories, environment variables,
remote output, and provider configuration as untrusted.

- Never concatenate untrusted values into `sh -c` or a remote command. For local
  execution, pass every argument separately through `Command`. Invoke a shell only
  when specification-defined shell semantics require it; document that trust
  boundary rather than inventing ad-hoc escaping.
- SSH necessarily crosses a shell boundary. Centralize remote quoting/encoding in
  the provider executor and test it adversarially. Never scatter interpolation of
  paths, container/image names, users, hosts, URLs, environment values, or
  repository content across modules.
- Validate names, ports, paths, URLs, image/Feature references, hosts, and users
  before execution when sshpod can provide a clearer failure than Podman or SSH.
  Malformed known schema fields fail; intentionally preserved unknown top-level
  fields remain inert until supported.
- Never log, fixture, commit, or expose passwords, tokens, private keys, or full
  sensitive environment values. Types containing secrets must not derive `Debug`
  unless output is safe; use redacted wrappers where appropriate.
- Treat repository paths as hostile. Validate containment where sshpod controls a
  destination and account for traversal, symlinks, and TOCTOU risk;
  canonicalization alone is not a complete defense.
- Use secure temporary-file APIs, never predictable `/tmp/sshpod-*` state. Create
  potentially sensitive files with restrictive permissions.
- Deserialization must not assume trusted shape or size. Bound externally
  controlled input where practical and stream large data instead of loading it
  whole.

## Processes and Resource Management

Manage every child process intentionally: check its exit status, choose explicit
stdin/stdout/stderr behavior, preserve safe command context on failure, prevent
leaks, and consider cancellation and cleanup. Never discard subprocess failures
unless documented as best-effort. Avoid needless filesystem or network calls and
set sensible bounds or timeouts for operations controlled by a repository or
remote host.

## Dependency Hygiene

Before adding a crate, confirm that `std` or an existing dependency is
insufficient. Evaluate maintenance activity, transitive-tree impact, unsafe code,
security history, and BSD-3-Clause-compatible licensing. Prefer focused crates to
large frameworks, but do not reimplement security-sensitive primitives when a
mature audited crate exists. Do not add a dependency to avoid a few straightforward
lines. Disable unnecessary default features and enable only what sshpod uses.
Every dependency change must pass `just deny` and `just ci` and include the lockfile
change.

## Diagnostics and Observability

Keep stable CLI output separate from diagnostics. Library modules must not add
arbitrary `println!` or `eprintln!`; use the established output path, and
centralize any future structured tracing. Diagnostics should identify the safe
provider, workspace, operation, subprocess, and remote stage needed to debug a
failure without logging credentials or secret values. Avoid noisy hot-loop logs.

## Testing

Non-trivial parsers, validators, normalizers, planners, and argument builders must
be unit-testable without Podman or SSH. Prefer pure functions for these stages.
Place focused unit tests beside code and public/subsystem tests in `tests/*.rs`;
name tests after behavior, such as `discovers_nested_configs`.

Every bug fix needs a regression test. Parser changes require valid and invalid
fixtures plus normalized-output assertions. Security-sensitive changes require
negative tests for relevant shell injection, hostile paths, malformed properties,
invalid ports, Unicode, empty values, conflicts, and argument boundaries.
Integration tests should exercise subsystem contracts. Live Podman/SSH tests must
be explicitly ignored or gated and must never be required for the default suite.
For performance-sensitive changes, test algorithmic behavior; do not add a
microbenchmark without a meaningful continuous use.

## Dev Container Compatibility

Treat the official specification, schema, and CLI behavior as references. Retain
unknown top-level properties for forward compatibility, but reject malformed
known properties. Preserve variable expressions until the dedicated resolution
stage. Never call functionality supported merely because it parses: distinguish
parsed, validated, normalized, planned, executable, and fully runtime-supported.
When schema behavior changes, update the model, validation, normalization, pinned
schema metadata, and conformance tests together.

## Commands and Definition of Done

- `cargo run -- --help`: build and exercise the CLI locally.
- `just fmt`: format Rust sources.
- `just test`: run format checks, compilation, tests, and strict Clippy.
- `just deny`: check advisories, licenses, bans, and sources.
- `just ci`: run the complete pre-submission validation suite.
- `just devcontainer-schema-check`: opt-in upstream schema drift check; review any
  diff before updating the pinned fixture.

Before declaring work complete: add proportional tests, run `just ci`, run the
schema check when Dev Container semantics change, document spec/security
decisions, and report any check that could not run. Tests must work without
installed Podman unless explicitly gated.

## Comments, Commits, and Pull Requests

Comments explain why, invariants, security decisions, spec deviations, or
interoperability constraints—not obvious code. Add useful rustdoc to public APIs
whose contracts are not self-evident and reference the relevant Dev Container
concept when behavior would otherwise look arbitrary.

Use short imperative commit subjects such as `Add YAML provider configuration`;
omit conventional-commit prefixes and co-author trailers. Pull requests must
explain the problem and resulting behavior, link relevant issues, list validation,
and state limitations or compatibility effects. Never include credentials,
private host details, or generated build artifacts.
