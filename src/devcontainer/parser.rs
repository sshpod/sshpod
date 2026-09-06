use std::{fs, path::Path};

use jsonc_parser::{ParseOptions, parse_to_serde_value};

use super::{
    discovery::ConfigCandidate,
    error::DevContainerError,
    model::{ConfigOrigin, ParsedDevContainer, RawDevContainer},
};

const JSONC_OPTIONS: ParseOptions = ParseOptions {
    allow_comments: true,
    allow_loose_object_property_names: false,
    allow_trailing_commas: false,
    allow_missing_commas: false,
    allow_single_quoted_strings: false,
    allow_hexadecimal_numbers: false,
    allow_unary_plus_numbers: false,
};

pub fn load(path: &Path) -> Result<ParsedDevContainer, DevContainerError> {
    let path = absolute_path(path)?;
    let contents = fs::read(&path).map_err(|source| DevContainerError::Io {
        path: path.clone(),
        source,
    })?;
    parse_bytes(ConfigOrigin::from_path(path, None), &contents)
}

/// Parse a candidate returned by [`super::discover`], retaining its workspace origin.
///
/// # Errors
///
/// Returns an I/O error when the candidate cannot be read, or a parse error for
/// invalid UTF-8, JSONC syntax, or known-property types.
pub fn load_candidate(
    candidate: &ConfigCandidate,
) -> Result<ParsedDevContainer, DevContainerError> {
    let contents = fs::read(&candidate.path).map_err(|source| DevContainerError::Io {
        path: candidate.path.clone(),
        source,
    })?;
    parse_bytes(
        ConfigOrigin::from_path(
            candidate.path.clone(),
            Some(candidate.workspace_root.clone()),
        ),
        &contents,
    )
}

/// Parse JSONC bytes using the supplied path and workspace context.
///
/// # Errors
///
/// Returns a parse error for invalid UTF-8, invalid JSONC, or a malformed known
/// Dev Container property.
pub fn parse_bytes(
    origin: ConfigOrigin,
    contents: &[u8],
) -> Result<ParsedDevContainer, DevContainerError> {
    let text = std::str::from_utf8(contents).map_err(|error| DevContainerError::Parse {
        path: origin.config_path.clone(),
        message: format!("configuration is not UTF-8: {error}"),
    })?;
    let document =
        parse_to_serde_value::<RawDevContainer>(text, &JSONC_OPTIONS).map_err(|error| {
            DevContainerError::Parse {
                path: origin.config_path.clone(),
                message: error.to_string(),
            }
        })?;
    Ok(ParsedDevContainer { origin, document })
}

fn absolute_path(path: &Path) -> Result<std::path::PathBuf, DevContainerError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let directory = std::env::current_dir().map_err(|source| DevContainerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(directory.join(path))
}
