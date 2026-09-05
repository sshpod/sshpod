use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

fn test_directory(name: &str) -> std::path::PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sshpod-cli-{}-{id}-{name}", process::id()))
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
    let directory = test_directory("providers");
    let config = directory.join("sshpod/config.yaml");
    let binary = env!("CARGO_BIN_EXE_sshpod");

    let added = Command::new(binary)
        .args([
            "provider",
            "add",
            "sandbox",
            "--type",
            "ssh",
            "--host",
            "sandbox",
            "--podman",
            "docker",
            "--ssh-arg=-A",
        ])
        .env("XDG_CONFIG_HOME", &directory)
        .output()?;
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let contents = fs::read_to_string(&config)?;
    assert!(contents.contains("sandbox:"));
    assert!(contents.contains("host: sandbox"));
    assert!(contents.contains("podman: docker"));
    assert!(contents.contains("sshArgs:"));

    let listed = Command::new(binary)
        .args(["provider", "list"])
        .env("XDG_CONFIG_HOME", &directory)
        .output()?;
    assert!(listed.status.success());
    assert!(String::from_utf8(listed.stdout)?.contains("sandbox\tssh\tsandbox\tdocker\t-"));

    let deleted = Command::new(binary)
        .args(["provider", "delete", "sandbox"])
        .env("XDG_CONFIG_HOME", &directory)
        .output()?;
    assert!(deleted.status.success());
    assert!(!fs::read_to_string(&config)?.contains("sandbox:"));
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn up_without_devcontainer_stops_before_podman_or_workspace_persistence() -> anyhow::Result<()> {
    let directory = test_directory("missing-devcontainer");
    let xdg = directory.join("xdg");
    let project = directory.join("project");
    let config = xdg.join("sshpod/config.yaml");
    fs::create_dir_all(
        config
            .parent()
            .ok_or_else(|| anyhow::anyhow!("config path has no parent"))?,
    )?;
    fs::create_dir_all(&project)?;
    fs::write(&config, "providers:\n  local:\n    type: local\n")?;

    let output = Command::new(env!("CARGO_BIN_EXE_sshpod"))
        .args(["up", "demo"])
        .env("XDG_CONFIG_HOME", &xdg)
        .env("PATH", "")
        .current_dir(&project)
        .output()?;

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("no Dev Container configuration found"));
    assert!(stderr.contains(".devcontainer/<folder>/devcontainer.json"));
    let persisted = fs::read_to_string(&config)?;
    assert!(!persisted.contains("workspaces:"));
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn up_persists_an_explicit_nested_devcontainer_selection() -> anyhow::Result<()> {
    let directory = test_directory("selected-devcontainer");
    let project = directory.join("project");
    let devcontainer = project.join(".devcontainer/rust/devcontainer.json");
    let binary_directory = directory.join("bin");
    let podman = binary_directory.join("podman");
    fs::create_dir_all(
        devcontainer.parent().ok_or_else(|| {
            anyhow::anyhow!("test Dev Container configuration path has no parent")
        })?,
    )?;
    fs::create_dir_all(&binary_directory)?;
    fs::write(&devcontainer, r#"{"image":"alpine"}"#)?;
    fs::write(
        &podman,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
    echo "podman version test"
elif [ "$1" = "container" ] && [ "$2" = "exists" ]; then
    exit 0
elif [ "$1" = "container" ] && [ "$2" = "inspect" ]; then
    echo "running"
else
    exit 64
fi
"#,
    )?;
    let mut permissions = fs::metadata(&podman)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&podman, permissions)?;

    let xdg = directory.join("xdg");
    let config = xdg.join("sshpod/config.yaml");
    fs::create_dir_all(
        config
            .parent()
            .ok_or_else(|| anyhow::anyhow!("config path has no parent"))?,
    )?;
    fs::write(&config, "providers:\n  local:\n    type: local\n")?;
    let output = Command::new(env!("CARGO_BIN_EXE_sshpod"))
        .args([
            "up",
            "demo",
            "--config",
            ".devcontainer/rust/devcontainer.json",
        ])
        .env("XDG_CONFIG_HOME", &xdg)
        .env("PATH", &binary_directory)
        .current_dir(&project)
        .output()?;

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let persisted = fs::read_to_string(&config)?;
    assert!(persisted.contains("workspaces:"));
    assert!(persisted.contains("devcontainer: .devcontainer/rust/devcontainer.json"));
    fs::remove_dir_all(directory)?;
    Ok(())
}
