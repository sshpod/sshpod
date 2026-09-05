use std::{
    fs, process,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

fn config_path(name: &str) -> std::path::PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sshpod-cli-{}-{id}-{name}.toml", process::id()))
}

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

#[test]
fn provider_add_list_and_delete_persist_configuration() -> anyhow::Result<()> {
    let config = config_path("providers");
    let binary = env!("CARGO_BIN_EXE_sshpod");

    let added = Command::new(binary)
        .args([
            "provider", "add", "sandbox", "--type", "ssh", "--host", "sandbox",
        ])
        .env("SSHPOD_CONFIG", &config)
        .output()?;
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let contents = fs::read_to_string(&config)?;
    assert!(contents.contains("[providers.sandbox]"));
    assert!(contents.contains("host = \"sandbox\""));

    let listed = Command::new(binary)
        .args(["provider", "list"])
        .env("SSHPOD_CONFIG", &config)
        .output()?;
    assert!(listed.status.success());
    assert!(String::from_utf8(listed.stdout)?.contains("sandbox\tssh\tsandbox"));

    let deleted = Command::new(binary)
        .args(["provider", "delete", "sandbox"])
        .env("SSHPOD_CONFIG", &config)
        .output()?;
    assert!(deleted.status.success());
    assert!(!fs::read_to_string(&config)?.contains("providers.sandbox"));
    fs::remove_file(config)?;
    Ok(())
}
