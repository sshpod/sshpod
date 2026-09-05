use std::process::Command;

#[test]
fn help_and_version_work_without_podman() -> anyhow::Result<()> {
    for (flag, expected) in [
        ("--help", "doctor"),
        ("-V", "sshpod 0.1.0\n"),
        (
            "--version",
            concat!("sshpod 0.1.0 - ", env!("SSHPOD_GIT_COMMIT"), "\n"),
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_sshpod"))
            .arg(flag)
            .env("PATH", "")
            .output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout)?;
        if flag == "--help" {
            assert!(stdout.contains(expected));
            assert!(!stdout.contains('\u{1b}'));
        } else {
            assert_eq!(stdout, expected);
        }
        assert!(output.stderr.is_empty());
    }
    Ok(())
}

#[test]
fn doctor_reports_missing_podman_on_stderr() -> anyhow::Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_sshpod"))
        .arg("doctor")
        .env("PATH", "")
        .output()?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("install Podman"));
    assert!(stderr.contains("PATH"));
    Ok(())
}
