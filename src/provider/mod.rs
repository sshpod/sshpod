mod executor;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub(crate) use executor::{CommandOutput, Executor};

/// A concrete place where sshpod invokes Podman.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Provider {
    Local,
    Ssh { host: String },
}

impl Provider {
    pub(crate) const fn kind(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ssh { .. } => "ssh",
        }
    }

    pub(crate) fn host(&self) -> Option<&str> {
        match self {
            Self::Local => None,
            Self::Ssh { host } => Some(host),
        }
    }

    pub(crate) fn validate(&self, name: &str) -> Result<()> {
        if let Self::Ssh { host } = self {
            ensure!(!host.is_empty(), "SSH provider {name:?} requires a host");
            ensure!(
                !host.starts_with('-')
                    && host.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'@')
                    }),
                "SSH host {host:?} contains unsupported characters"
            );
        }
        Ok(())
    }
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
