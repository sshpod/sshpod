mod executor;

use std::{borrow::Cow, path::PathBuf};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub(crate) use executor::{CommandOutput, Executor};

/// A concrete place where sshpod invokes Podman.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Provider {
    Local {
        #[serde(
            default = "default_local_podman",
            skip_serializing_if = "is_default_local_podman"
        )]
        podman: PathBuf,
    },
    Ssh {
        host: String,
        #[serde(
            default = "default_ssh_podman",
            skip_serializing_if = "is_default_ssh_podman"
        )]
        podman: String,
        #[serde(default, rename = "sshArgs", skip_serializing_if = "Vec::is_empty")]
        ssh_args: Vec<String>,
    },
}

impl Provider {
    pub(crate) const fn kind(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Ssh { .. } => "ssh",
        }
    }

    pub(crate) fn host(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::Ssh { host, .. } => Some(host),
        }
    }

    pub(crate) fn podman(&self) -> Cow<'_, str> {
        match self {
            Self::Local { podman } => podman.to_string_lossy(),
            Self::Ssh { podman, .. } => Cow::Borrowed(podman),
        }
    }

    pub(crate) fn validate(&self, name: &str) -> Result<()> {
        match self {
            Self::Local { podman } => ensure!(
                !podman.as_os_str().is_empty(),
                "local provider {name:?} requires a non-empty podman command"
            ),
            Self::Ssh {
                host,
                podman,
                ssh_args,
            } => {
                ensure!(
                    !host.trim().is_empty(),
                    "SSH provider {name:?} requires \"host\""
                );
                ensure!(
                    !host.starts_with('-')
                        && host.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric()
                                || matches!(byte, b'.' | b'_' | b'-' | b'@')
                        }),
                    "SSH host {host:?} contains unsupported characters"
                );
                ensure!(
                    !podman.trim().is_empty(),
                    "SSH provider {name:?} requires a non-empty podman command"
                );
                ensure!(
                    ssh_args.iter().all(|argument| !argument.trim().is_empty()),
                    "SSH provider {name:?} contains an empty SSH argument"
                );
            }
        }
        Ok(())
    }
}

pub(crate) fn default_local_podman() -> PathBuf {
    PathBuf::from("podman")
}

pub(crate) fn default_ssh_podman() -> String {
    "podman".to_owned()
}

fn is_default_local_podman(value: &PathBuf) -> bool {
    value == &default_local_podman()
}

fn is_default_ssh_podman(value: &str) -> bool {
    value == "podman"
}

pub(crate) fn validate_name(name: &str, kind: &str) -> Result<()> {
    ensure!(
        (1..=48).contains(&name.len())
            && name
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            && name.bytes().all(|byte| byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-')),
        "{kind} name {name:?} must be 1-48 lowercase letters, digits, dots, underscores, or hyphens and start with a letter or digit"
    );
    Ok(())
}
