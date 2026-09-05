use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn packaged_commit(manifest_dir: &Path) -> Option<String> {
    let path = manifest_dir.join(".cargo_vcs_info.json");
    println!("cargo:rerun-if-changed={}", path.display());
    let contents = fs::read_to_string(path).ok()?;
    let (_, value) = contents.split_once("\"sha1\"")?;
    let (_, value) = value.split_once(':')?;
    let value = value.trim_start().strip_prefix('"')?;
    let (commit, _) = value.split_once('"')?;
    if matches!(commit.len(), 40 | 64) && commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(commit.to_owned())
    } else {
        None
    }
}

fn git(directory: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned())
}

fn git_commit(manifest_dir: &Path) -> Option<String> {
    if !manifest_dir.join(".git").exists() {
        return None;
    }

    // Resolve paths through Git to support both normal checkouts and worktrees.
    for name in ["HEAD", "refs", "packed-refs", "logs/HEAD"] {
        if let Some(path) = git(manifest_dir, &["rev-parse", "--git-path", name]) {
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                manifest_dir.join(path)
            };
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }

    git(manifest_dir, &["rev-parse", "--verify", "HEAD"])
}

fn main() {
    // Cargo can reuse a target directory between checkout and package builds.
    println!("cargo:rerun-if-env-changed=CARGO_MANIFEST_DIR");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git");

    // Do not accidentally use a parent repository when building a source archive.
    let commit = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .as_deref()
        .and_then(|manifest_dir| {
            packaged_commit(manifest_dir).or_else(|| git_commit(manifest_dir))
        });
    println!(
        "cargo:rustc-env=SSHPOD_GIT_COMMIT={}",
        commit.as_deref().unwrap_or("unknown")
    );
}
