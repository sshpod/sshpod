use std::{fs, path::Path, process, process::Command};

struct Cleanup {
    container: String,
    directory: std::path::PathBuf,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _result = Command::new("podman")
            .args(["rm", "--force", &self.container])
            .output();
        let _result = fs::remove_dir_all(&self.directory);
    }
}

fn run(binary: &str, xdg: &Path, directory: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(binary)
        .args(args)
        .env("XDG_CONFIG_HOME", xdg)
        .current_dir(directory)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "sshpod {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).map_err(Into::into)
}

#[test]
#[ignore = "requires a working local Podman runtime and the Alpine image or network access"]
fn local_image_workspace_vertical_slice() -> anyhow::Result<()> {
    let workspace = format!("live-{}", process::id());
    let container = format!("sshpod-{workspace}-local");
    let directory = std::env::temp_dir().join(format!("sshpod-{workspace}"));
    let xdg = directory.join("xdg");
    let project = directory.join("project");
    let devcontainer = project.join(".devcontainer/devcontainer.json");
    fs::create_dir_all(
        devcontainer
            .parent()
            .ok_or_else(|| anyhow::anyhow!("test devcontainer path has no parent"))?,
    )?;
    fs::write(
        &devcontainer,
        r#"{
            // Exercise JSONC and a lifecycle command.
            "image": "docker.io/library/alpine:latest",
            "postCreateCommand": ["sh", "-c", "test -d ."]
        }"#,
    )?;
    let _cleanup = Cleanup {
        container,
        directory,
    };
    let binary = env!("CARGO_BIN_EXE_sshpod");

    run(
        binary,
        &xdg,
        &project,
        &["provider", "add", "local", "--type", "local"],
    )?;
    run(binary, &xdg, &project, &["up", &workspace])?;
    let listed = run(binary, &xdg, &project, &[])?;
    anyhow::ensure!(listed.contains(&format!("{workspace}\tlocal\trunning:local")));
    run(binary, &xdg, &project, &["down", &workspace])?;
    Ok(())
}
