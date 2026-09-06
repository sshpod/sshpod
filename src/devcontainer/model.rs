use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use super::{
    command::LifecycleCommand,
    error::Diagnostic,
    mount::{Mount, RawMount},
    port::{AppPort, ForwardPort, PortAttributes, RawForwardPort},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    pub fn into_vec(self) -> Vec<T> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigOrigin {
    pub config_path: PathBuf,
    pub config_dir: PathBuf,
    pub workspace_root: Option<PathBuf>,
}

impl ConfigOrigin {
    pub fn from_path(path: PathBuf, workspace_root: Option<PathBuf>) -> Self {
        let config_dir = path
            .parent()
            .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
        Self {
            config_path: path,
            config_dir,
            workspace_root,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawBuild {
    pub dockerfile: Option<String>,
    pub context: Option<String>,
    pub target: Option<String>,
    #[serde(default)]
    pub args: BTreeMap<String, String>,
    pub cache_from: Option<OneOrMany<String>>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum ShutdownAction {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "stopContainer")]
    StopContainer,
    #[serde(rename = "stopCompose")]
    StopCompose,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
pub enum WaitFor {
    #[serde(rename = "initializeCommand")]
    Initialize,
    #[serde(rename = "onCreateCommand")]
    OnCreate,
    #[serde(rename = "updateContentCommand")]
    #[default]
    UpdateContent,
    #[serde(rename = "postCreateCommand")]
    PostCreate,
    #[serde(rename = "postStartCommand")]
    PostStart,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum UserEnvProbe {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "loginShell")]
    LoginShell,
    #[serde(rename = "loginInteractiveShell")]
    LoginInteractiveShell,
    #[serde(rename = "interactiveShell")]
    InteractiveShell,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum GpuRequirement {
    Boolean(bool),
    Name(String),
    Detailed(GpuDetails),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct GpuDetails {
    pub cores: Option<u64>,
    pub memory: Option<String>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct HostRequirements {
    pub cpus: Option<u64>,
    pub memory: Option<String>,
    pub storage: Option<String>,
    pub gpu: Option<GpuRequirement>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretMetadata {
    pub description: Option<String>,
    pub documentation_url: Option<String>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawDevContainer {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,
    pub name: Option<String>,

    pub image: Option<String>,
    pub build: Option<RawBuild>,
    #[serde(rename = "dockerFile")]
    pub docker_file: Option<String>,
    pub context: Option<String>,
    pub docker_compose_file: Option<OneOrMany<String>>,
    pub service: Option<String>,
    pub run_services: Option<Vec<String>>,

    pub run_args: Option<Vec<String>>,
    pub override_command: Option<bool>,
    pub shutdown_action: Option<ShutdownAction>,
    pub init: Option<bool>,
    pub privileged: Option<bool>,
    #[serde(default)]
    pub cap_add: Vec<String>,
    #[serde(default)]
    pub security_opt: Vec<String>,

    pub workspace_folder: Option<String>,
    pub workspace_mount: Option<String>,
    #[serde(default)]
    pub mounts: Vec<RawMount>,

    #[serde(default)]
    pub container_env: BTreeMap<String, String>,
    #[serde(default)]
    pub remote_env: BTreeMap<String, Option<String>>,
    pub container_user: Option<String>,
    pub remote_user: Option<String>,
    #[serde(rename = "updateRemoteUserUID")]
    pub update_remote_user_uid: Option<bool>,
    pub user_env_probe: Option<UserEnvProbe>,

    #[serde(default)]
    pub forward_ports: Vec<RawForwardPort>,
    #[serde(default)]
    pub ports_attributes: IndexMap<String, PortAttributes>,
    pub other_ports_attributes: Option<PortAttributes>,
    pub app_port: Option<OneOrMany<AppPort>>,

    pub initialize_command: Option<LifecycleCommand>,
    pub on_create_command: Option<LifecycleCommand>,
    pub update_content_command: Option<LifecycleCommand>,
    pub post_create_command: Option<LifecycleCommand>,
    pub post_start_command: Option<LifecycleCommand>,
    pub post_attach_command: Option<LifecycleCommand>,
    pub wait_for: Option<WaitFor>,

    #[serde(default)]
    pub features: IndexMap<String, Value>,
    #[serde(default)]
    pub override_feature_install_order: Vec<String>,
    pub host_requirements: Option<HostRequirements>,
    #[serde(default)]
    pub customizations: IndexMap<String, Value>,
    #[serde(default)]
    pub secrets: IndexMap<String, SecretMetadata>,
    #[serde(rename = "additionalProperties")]
    pub additional_properties: Option<IndexMap<String, Value>>,

    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedDevContainer {
    pub origin: ConfigOrigin,
    pub document: RawDevContainer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildConfig {
    pub dockerfile: String,
    pub context: String,
    pub target: Option<String>,
    pub args: BTreeMap<String, String>,
    pub cache_from: Vec<String>,
    pub options: Vec<String>,
    /// Additional legacy `build` options allowed by the upstream base schema.
    pub extensions: IndexMap<String, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeConfig {
    pub files: Vec<String>,
    pub service: String,
    /// `None` means the Compose implementation should start all services.
    pub run_services: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContainerSource {
    Unspecified,
    Image(String),
    Build(BuildConfig),
    Compose(ComposeConfig),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeConfig {
    pub run_args: Vec<String>,
    pub override_command: Option<bool>,
    pub shutdown_action: Option<ShutdownAction>,
    pub init: bool,
    pub privileged: bool,
    pub cap_add: Vec<String>,
    pub security_opt: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceConfig {
    pub folder: Option<String>,
    pub mount: Option<String>,
    pub mounts: Vec<Mount>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentConfig {
    pub container: BTreeMap<String, String>,
    pub remote: BTreeMap<String, Option<String>>,
    pub container_user: Option<String>,
    pub remote_user: Option<String>,
    /// `None` retains the spec's context-dependent default (enabled for local folders).
    pub update_remote_user_uid: Option<bool>,
    pub user_env_probe: UserEnvProbe,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PortsConfig {
    pub forward: Vec<ForwardPort>,
    pub attributes: IndexMap<String, PortAttributes>,
    pub other_attributes: Option<PortAttributes>,
    pub app: Vec<AppPort>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LifecycleConfig {
    pub initialize: Option<LifecycleCommand>,
    pub on_create: Option<LifecycleCommand>,
    pub update_content: Option<LifecycleCommand>,
    pub post_create: Option<LifecycleCommand>,
    pub post_start: Option<LifecycleCommand>,
    pub post_attach: Option<LifecycleCommand>,
    pub wait_for: WaitFor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feature {
    pub reference: String,
    pub options: Value,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FeaturesConfig {
    pub declarations: Vec<Feature>,
    pub override_install_order: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetadataConfig {
    pub schema: Option<String>,
    pub name: Option<String>,
    pub customizations: IndexMap<String, Value>,
    pub secrets: IndexMap<String, SecretMetadata>,
    pub additional_properties: Option<IndexMap<String, Value>>,
    pub extensions: IndexMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedDevContainer {
    pub origin: ConfigOrigin,
    pub source: ContainerSource,
    pub runtime: RuntimeConfig,
    pub workspace: WorkspaceConfig,
    pub environment: EnvironmentConfig,
    pub ports: PortsConfig,
    pub lifecycle: LifecycleConfig,
    pub features: FeaturesConfig,
    pub host_requirements: Option<HostRequirements>,
    pub metadata: MetadataConfig,
    pub explicitly_set: BTreeSet<&'static str>,
    pub diagnostics: Vec<Diagnostic>,
}
