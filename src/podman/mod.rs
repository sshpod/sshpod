//! Podman CLI integration.

mod commands;

use anyhow::{Context, Result, bail, ensure};
use std::process::{Command, Output, Stdio};

pub(crate) use commands::{
    ContainerSpec, build_image, check_available, container_status, create, exec, start, stop,
};

/// Execute `podman --version` and return Podman's version text.
///
/// # Errors
///
/// Returns an error when Podman cannot be executed, exits unsuccessfully, or
/// returns empty or invalid output.
pub fn version() -> Result<String> {
    version_with(&mut Command::new("podman"))
}

fn version_with(command: &mut Command) -> Result<String> {
    let output = command.arg("--version").stdin(Stdio::null()).output()
        .context("could not execute `podman --version`; install Podman and ensure `podman` is on PATH and executable")?;
    version_output(output)
}

fn version_output(output: Output) -> Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`podman --version` failed ({}): {}. Run `podman --version` directly and repair the local Podman installation",
            output.status,
            if stderr.trim().is_empty() {
                "no diagnostic output"
            } else {
                stderr.trim()
            }
        );
    }
    let stdout =
        String::from_utf8(output.stdout).context("`podman --version` returned invalid UTF-8")?;
    let version = stdout.trim();
    ensure!(
        !version.is_empty(),
        "`podman --version` returned empty output; check the local Podman installation"
    );
    // Preserve Podman's version text, including distribution-specific suffixes.
    Ok(version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::version_with;
    use std::process::Command;

    #[test]
    fn missing_executable_has_actionable_context() -> anyhow::Result<()> {
        let missing = std::env::current_exe()?.join("nonexistent-podman");
        let error = version_with(&mut Command::new(missing))
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected execution failure"))?;
        let message = format!("{error:#}");
        assert!(message.contains("podman --version"));
        assert!(message.contains("PATH and executable"));
        Ok(())
    }

    #[cfg(unix)]
    mod output {
        use super::super::version_output;
        use std::os::unix::process::ExitStatusExt;
        use std::process::{ExitStatus, Output};

        fn output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
            Output {
                status: ExitStatus::from_raw(code << 8),
                stdout: stdout.to_vec(),
                stderr: stderr.to_vec(),
            }
        }

        #[test]
        fn preserves_version_and_distribution_suffix() -> anyhow::Result<()> {
            assert_eq!(
                version_output(output(0, b"podman version 5.6.0-dev\n", b""))?,
                "podman version 5.6.0-dev"
            );
            Ok(())
        }

        #[test]
        fn rejects_empty_and_invalid_output() {
            assert!(version_output(output(0, b" \n", b"")).is_err());
            assert!(version_output(output(0, &[0xff], b"")).is_err());
        }

        #[test]
        fn reports_status_and_diagnostics() -> anyhow::Result<()> {
            for (stderr, diagnostic) in [
                (b"broken installation".as_slice(), "broken installation"),
                (b"".as_slice(), "no diagnostic output"),
            ] {
                let error = version_output(output(42, b"", stderr))
                    .err()
                    .ok_or_else(|| anyhow::anyhow!("expected command failure"))?;
                let message = error.to_string();
                assert!(message.contains("42"));
                assert!(message.contains(diagnostic));
                assert!(message.contains("repair"));
            }
            Ok(())
        }
    }
}
