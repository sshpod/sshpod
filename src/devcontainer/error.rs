use std::{error::Error, fmt, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Warning,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Warning => formatter.write_str("warning"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    pub fn warning(
        path: impl Into<String>,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            path: path.into(),
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug)]
pub enum DevContainerError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    Discovery {
        path: PathBuf,
        message: String,
    },
    Validation(ValidationErrors),
}

impl fmt::Display for DevContainerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::Parse { path, message } => {
                write!(formatter, "failed to parse {}: {message}", path.display())
            }
            Self::Discovery { path, message } => {
                write!(
                    formatter,
                    "failed to discover Dev Container configurations in {}: {message}",
                    path.display()
                )
            }
            Self::Validation(errors) => errors.fmt(formatter),
        }
    }
}

impl Error for DevContainerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Validation(errors) => Some(errors),
            Self::Parse { .. } | Self::Discovery { .. } => None,
        }
    }
}

impl From<ValidationErrors> for DevContainerError {
    fn from(value: ValidationErrors) -> Self {
        Self::Validation(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationErrors {
    pub config_path: PathBuf,
    pub issues: Vec<ValidationIssue>,
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid Dev Container configuration {}",
            self.config_path.display()
        )?;
        for issue in &self.issues {
            write!(formatter, "; {}: {}", issue.path, issue.message)?;
        }
        Ok(())
    }
}

impl Error for ValidationErrors {}
