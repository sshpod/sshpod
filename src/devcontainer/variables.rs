use std::env;

use anyhow::{Context, Result, bail};

/// Resolve the small variable subset consumed by sshpod's current runtime.
///
/// Parsing never calls this function: the normalized configuration retains the
/// original expressions so a future runtime planner can resolve the complete
/// Dev Container variable vocabulary with the correct phase-specific context.
pub(crate) fn substitute(
    value: &str,
    local_workspace: &str,
    workspace_folder: &str,
    workspace_basename: &str,
) -> Result<String> {
    let mut output = String::new();
    let mut remaining = value;
    while let Some((before, variable)) = remaining.split_once("${") {
        output.push_str(before);
        let (variable, after) = variable
            .split_once('}')
            .with_context(|| format!("unterminated devcontainer variable in {value:?}"))?;
        let replacement = match variable {
            "localWorkspaceFolder" => local_workspace.to_owned(),
            "localWorkspaceFolderBasename" => workspace_basename.to_owned(),
            "containerWorkspaceFolder" => workspace_folder.to_owned(),
            "containerWorkspaceFolderBasename" => workspace_folder
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or(workspace_basename)
                .to_owned(),
            _ => {
                if let Some(name) = variable
                    .strip_prefix("localEnv:")
                    .or_else(|| variable.strip_prefix("env:"))
                {
                    env::var(name).with_context(|| {
                        format!("environment variable {name:?} used by devcontainer is not set")
                    })?
                } else {
                    bail!(
                        "devcontainer variable ${{{variable}}} is not supported by the runtime yet"
                    );
                }
            }
        };
        output.push_str(&replacement);
        remaining = after;
    }
    output.push_str(remaining);
    Ok(output)
}
