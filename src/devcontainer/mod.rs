//! Discovery, strict JSONC parsing, validation, and normalization for the
//! Development Container specification.
//!
//! Parsing deliberately does not resolve variables or create containers. Use
//! [`parse`] when callers need a separate validation step, or [`load`] for the
//! compact parse-and-validate API.

mod command;
mod discovery;
mod error;
mod model;
mod mount;
mod parser;
mod port;
mod validation;
mod variables;

use std::path::Path;

pub use command::{Command as LifecycleCommandValue, LifecycleCommand};
pub use discovery::{
    ConfigCandidate, ConfigLocation, NESTED_CONFIG_PATTERN, PRIMARY_CONFIG, ROOT_CONFIG, discover,
};
pub use error::{
    DevContainerError, Diagnostic, DiagnosticSeverity, ValidationErrors, ValidationIssue,
};
pub use model::{
    BuildConfig, ComposeConfig, ConfigOrigin, ContainerSource, EnvironmentConfig, Feature,
    FeaturesConfig, GpuDetails, GpuRequirement, HostRequirements, LifecycleConfig, MetadataConfig,
    NormalizedDevContainer, OneOrMany, ParsedDevContainer, PortsConfig, RawBuild, RawDevContainer,
    RuntimeConfig, SecretMetadata, ShutdownAction, UserEnvProbe, WaitFor, WorkspaceConfig,
};
pub use mount::{Mount, MountType, RawMount, RawMountObject};
pub use parser::{load_candidate, parse_bytes};
pub use port::{
    AppPort, AutoForwardAction, ForwardPort, PortAttributes, PortProtocol, RawForwardPort,
};
pub(crate) use variables::substitute;

/// Parse JSONC into the schema-shaped model without applying semantic defaults.
///
/// # Errors
///
/// Returns an I/O error when the path cannot be read, or a parse error for
/// invalid UTF-8, JSONC syntax, or known-property types.
pub fn parse(path: impl AsRef<Path>) -> Result<ParsedDevContainer, DevContainerError> {
    parser::load(path.as_ref())
}

/// Parse, validate, and normalize an explicitly selected configuration.
///
/// # Errors
///
/// Returns an I/O or parse error from [`parse`], or aggregate semantic
/// validation errors when the document conflicts with the official schema.
pub fn load(path: impl AsRef<Path>) -> Result<NormalizedDevContainer, DevContainerError> {
    Ok(parse(path)?.validate()?)
}

pub(crate) fn config_relative_paths() -> [&'static str; 2] {
    [PRIMARY_CONFIG, ROOT_CONFIG]
}
