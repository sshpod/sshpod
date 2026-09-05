# GitHub automation

- `ci.yml`: runs on branch pushes, pull requests, and manual dispatch; coordinates
  tests, security checks, coverage, and release builds.
- `test.yml`: reusable formatting, checking, strict Clippy, package verification,
  Linux/macOS/Windows tests, and an MSRV test using Cargo.toml.
- `build.yml`: reusable release builds for Linux x86_64 musl, Apple Silicon macOS,
  and Windows x86_64. Checks help and both version formats, then stores binary
  archives with the README and license as GitHub Actions artifacts.
- `coverage.yml`: generates an LCOV artifact using grcov; no external
  coverage service or token is needed.
- `security-audit.yml`: cargo-audit and cargo-deny, called from CI and also run
  daily or manually. Scheduled runs use the repository's default branch.
- `release.yml`: manual release preparation; runs tests, security checks, builds,
  package verification, and a publish dry run. It uploads review artifacts only.

All workflows use read-only repository permissions. None creates tags or GitHub
releases, or publishes to crates.io. No repository secrets are required. Artifacts
expire after 14 days. Publication automation can be added when releases are ready.

Coverage and audit tools are installed directly with `cargo install`, following
the cron-when conventions. cargo-audit is version-pinned and cached.

Dependabot checks Cargo and GitHub Actions weekly. Container testing, telemetry
matrices, RPM/DEB packaging, and template-copy instructions are intentionally absent.

Contribution guidance and issue/PR templates cover the current project scope.
See [SECURITY.md](SECURITY.md) for private vulnerability reporting.
