use std::{path::Path, process::Command};

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned())
}

fn main() {
    // Cargo can reuse a target directory between checkout and package builds.
    println!("cargo:rerun-if-env-changed=CARGO_MANIFEST_DIR");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git");

    // Do not accidentally use a parent repository when building a source archive.
    let commit = if Path::new(".git").exists() {
        // Resolve paths through Git to support both normal checkouts and worktrees.
        for name in ["HEAD", "refs", "packed-refs", "logs/HEAD"] {
            if let Some(path) = git(&["rev-parse", "--git-path", name])
                && Path::new(&path).exists()
            {
                println!("cargo:rerun-if-changed={path}");
            }
        }
        git(&["rev-parse", "--verify", "HEAD"])
    } else {
        None
    };
    println!(
        "cargo:rustc-env=SSHPOD_GIT_COMMIT={}",
        commit.as_deref().unwrap_or("unknown")
    );
}
