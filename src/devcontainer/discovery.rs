use std::{fs, path::PathBuf};

use super::error::DevContainerError;

pub const PRIMARY_CONFIG: &str = ".devcontainer/devcontainer.json";
pub const ROOT_CONFIG: &str = ".devcontainer.json";
pub const NESTED_CONFIG_PATTERN: &str = ".devcontainer/<folder>/devcontainer.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigLocation {
    Primary,
    Root,
    Nested { folder: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigCandidate {
    pub workspace_root: PathBuf,
    pub relative_path: String,
    pub path: PathBuf,
    pub location: ConfigLocation,
}

/// Find all Dev Container configuration locations defined for a workspace.
///
/// # Errors
///
/// Returns an error if the workspace cannot be resolved or its `.devcontainer`
/// directory cannot be inspected.
pub fn discover(
    workspace_root: &std::path::Path,
) -> Result<Vec<ConfigCandidate>, DevContainerError> {
    let root = workspace_root
        .canonicalize()
        .map_err(|source| DevContainerError::Io {
            path: workspace_root.to_path_buf(),
            source,
        })?;
    let mut candidates = Vec::new();
    for (relative_path, location) in [
        (PRIMARY_CONFIG, ConfigLocation::Primary),
        (ROOT_CONFIG, ConfigLocation::Root),
    ] {
        let path = root.join(relative_path);
        if path.is_file() {
            candidates.push(ConfigCandidate {
                workspace_root: root.clone(),
                relative_path: relative_path.to_owned(),
                path,
                location,
            });
        }
    }

    let nested_root = root.join(".devcontainer");
    if !nested_root.is_dir() {
        return Ok(candidates);
    }
    let entries = fs::read_dir(&nested_root).map_err(|source| DevContainerError::Io {
        path: nested_root.clone(),
        source,
    })?;
    let mut nested = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| DevContainerError::Discovery {
            path: nested_root.clone(),
            message: error.to_string(),
        })?;
        if !entry.path().join("devcontainer.json").is_file() {
            continue;
        }
        let folder = entry
            .file_name()
            .into_string()
            .map_err(|_| DevContainerError::Discovery {
                path: nested_root.clone(),
                message: "configuration folder name is not valid UTF-8".to_owned(),
            })?;
        let relative_path = format!(".devcontainer/{folder}/devcontainer.json");
        nested.push(ConfigCandidate {
            workspace_root: root.clone(),
            path: root.join(&relative_path),
            relative_path,
            location: ConfigLocation::Nested { folder },
        });
    }
    nested.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    candidates.extend(nested);
    Ok(candidates)
}
