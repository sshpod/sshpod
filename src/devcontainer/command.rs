use indexmap::IndexMap;
use serde::Deserialize;

/// A command that is executed either through a shell or directly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum Command {
    Shell(String),
    Exec(Vec<String>),
}

/// A Dev Container lifecycle command in any schema-supported representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum LifecycleCommand {
    Shell(String),
    Exec(Vec<String>),
    Parallel(IndexMap<String, Command>),
}
