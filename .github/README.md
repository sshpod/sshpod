# GitHub automation

- `ci.yml`: runs on branch pushes, pull requests, and manual dispatch; coordinates
  tests, security checks, coverage, and release builds.
- `test.yml`: reusable formatting, checking, strict Clippy, package verification,
  Linux/macOS tests, and an MSRV test using Cargo.toml.
- `build.yml`: reusable release builds for Linux musl and macOS, each on native
  x86_64 (amd64) and aarch64 (arm64) runners. Checks help and both version formats,
  then stores binary
  archives with the README and license as GitHub Actions artifacts.
- `coverage.yml`: generates an LCOV artifact using grcov; no external
  coverage service or token is needed.
- `security-audit.yml`: cargo-audit and cargo-deny, called from CI and also run
  daily or manually. Scheduled runs use the repository's default branch.
- `release.yml`: follows cron-when's deployment workflow, running tests and builds
  on every tag push or manual dispatch. Builds Linux musl and macOS for both
  x86_64 (amd64) and aarch64 (arm64), including Linux RPM/DEB packages.
  Tags starting with `t` only test and build. Other tags create a GitHub release;
  tag pushes also publish to crates.io using the `CRATES_TOKEN` repository secret.
  Keep the tag and Cargo.toml version aligned; this is not automatically enforced.
  Manual dispatch on a branch only prepares artifacts; on a release tag it can
  create a GitHub release but does not publish to crates.io.

CI uses read-only repository permissions; Deploy uses `contents: write` like the
reference. CI needs no secrets; publishing needs `CRATES_TOKEN`.
Actions artifacts expire after 14 days; release assets remain attached to releases.
The release workflow must be committed before creating and pushing the tag.
Adding this workflow does not retroactively trigger deployment for existing tags.

Coverage and audit tools are installed directly with `cargo install`, following
the cron-when conventions. cargo-audit is version-pinned and cached.

Dependabot checks Cargo and GitHub Actions weekly. Container testing, telemetry
matrices, and template-copy instructions are intentionally absent.

Contribution guidance and issue/PR templates cover the current project scope.
See [SECURITY.md](SECURITY.md) for private vulnerability reporting.
