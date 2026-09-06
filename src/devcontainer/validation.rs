use std::collections::BTreeSet;

use super::{
    error::{Diagnostic, ValidationErrors, ValidationIssue},
    model::{
        BuildConfig, ComposeConfig, ContainerSource, EnvironmentConfig, Feature, FeaturesConfig,
        LifecycleConfig, MetadataConfig, NormalizedDevContainer, ParsedDevContainer, PortsConfig,
        RuntimeConfig, ShutdownAction, UserEnvProbe, WaitFor, WorkspaceConfig,
    },
    mount::{Mount, RawMount},
    port::{AutoForwardAction, ForwardPort, RawForwardPort},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceKind {
    Unspecified,
    Image,
    Build,
    Compose,
}

impl ParsedDevContainer {
    /// Validate schema-level relationships and produce runtime-oriented types.
    ///
    /// # Errors
    ///
    /// Returns all detected semantic validation issues, including conflicting
    /// sources, invalid ports, invalid mounts, and closed-object properties.
    pub fn validate(self) -> Result<NormalizedDevContainer, ValidationErrors> {
        let mut issues = Vec::new();
        let document = &self.document;

        validate_closed_objects(document, &mut issues);
        validate_ports(document, &mut issues);
        validate_host_requirements(document, &mut issues);
        validate_secrets(document, &mut issues);
        validate_mounts(document, &mut issues);

        let has_image = document.image.is_some();
        let has_build = document.build.is_some()
            || document.docker_file.is_some()
            || document.context.is_some();
        let has_compose = document.docker_compose_file.is_some();
        let source_count =
            usize::from(has_image) + usize::from(has_build) + usize::from(has_compose);
        if source_count > 1 {
            issue(
                &mut issues,
                "/",
                "`image`, Dockerfile/build, and `dockerComposeFile` are mutually exclusive",
            );
        }

        let source_kind = if has_image {
            SourceKind::Image
        } else if has_build {
            SourceKind::Build
        } else if has_compose {
            SourceKind::Compose
        } else {
            SourceKind::Unspecified
        };

        validate_source(document, source_kind, &mut issues);

        if !issues.is_empty() {
            return Err(ValidationErrors {
                config_path: self.origin.config_path,
                issues,
            });
        }

        Ok(normalize(self, source_kind))
    }
}

fn normalize(parsed: ParsedDevContainer, source_kind: SourceKind) -> NormalizedDevContainer {
    let document = &parsed.document;
    let diagnostics = normalization_diagnostics(document);
    let explicitly_set = explicit_properties(document);
    let document = parsed.document;
    let source = normalize_source(&document, source_kind);
    let is_compose = matches!(source, ContainerSource::Compose(_));
    let has_source = !matches!(source, ContainerSource::Unspecified);

    let mounts = document
        .mounts
        .into_iter()
        .filter_map(normalize_mount)
        .collect();
    let forward = document
        .forward_ports
        .iter()
        .filter_map(|port| normalize_forward_port(port).ok())
        .collect();
    let app = document
        .app_port
        .map_or_else(Vec::new, super::model::OneOrMany::into_vec);
    let features = document
        .features
        .into_iter()
        .map(|(reference, options)| Feature { reference, options })
        .collect();

    NormalizedDevContainer {
        origin: parsed.origin,
        source,
        runtime: RuntimeConfig {
            run_args: document.run_args.unwrap_or_default(),
            override_command: document
                .override_command
                .or_else(|| has_source.then_some(!is_compose)),
            shutdown_action: document.shutdown_action.or_else(|| {
                has_source.then_some(if is_compose {
                    ShutdownAction::StopCompose
                } else {
                    ShutdownAction::StopContainer
                })
            }),
            init: document.init.unwrap_or(false),
            privileged: document.privileged.unwrap_or(false),
            cap_add: document.cap_add,
            security_opt: document.security_opt,
        },
        workspace: WorkspaceConfig {
            folder: document.workspace_folder,
            mount: document.workspace_mount,
            mounts,
        },
        environment: EnvironmentConfig {
            container: document.container_env,
            remote: document.remote_env,
            container_user: document.container_user,
            remote_user: document.remote_user,
            update_remote_user_uid: document.update_remote_user_uid,
            user_env_probe: document
                .user_env_probe
                .unwrap_or(UserEnvProbe::LoginInteractiveShell),
        },
        ports: PortsConfig {
            forward,
            attributes: document.ports_attributes,
            other_attributes: document.other_ports_attributes,
            app,
        },
        lifecycle: LifecycleConfig {
            initialize: document.initialize_command,
            on_create: document.on_create_command,
            update_content: document.update_content_command,
            post_create: document.post_create_command,
            post_start: document.post_start_command,
            post_attach: document.post_attach_command,
            wait_for: document.wait_for.unwrap_or(WaitFor::UpdateContent),
        },
        features: FeaturesConfig {
            declarations: features,
            override_install_order: document.override_feature_install_order,
        },
        host_requirements: document.host_requirements,
        metadata: MetadataConfig {
            schema: document.schema,
            name: document.name,
            customizations: document.customizations,
            secrets: document.secrets,
            additional_properties: document.additional_properties,
            extensions: document.extra,
        },
        explicitly_set,
        diagnostics,
    }
}

fn normalization_diagnostics(document: &super::model::RawDevContainer) -> Vec<Diagnostic> {
    let mut diagnostics = document
        .extra
        .keys()
        .map(|property| {
            Diagnostic::warning(
                format!("/{}", json_pointer_escape(property)),
                "unknown-property",
                format!(
                    "unknown top-level property {property:?} was preserved for forward compatibility"
                ),
            )
        })
        .collect::<Vec<_>>();
    if document.docker_file.is_some() {
        diagnostics.push(Diagnostic::warning(
            "/dockerFile",
            "legacy-dockerfile",
            "legacy `dockerFile` syntax was normalized to `build.dockerfile`",
        ));
        if document
            .build
            .as_ref()
            .is_some_and(|build| build.context.is_some())
        {
            diagnostics.push(Diagnostic::warning(
                "/build/context",
                "ignored-legacy-build-context",
                "the reference implementation uses top-level `context` with legacy `dockerFile`; `build.context` was preserved but is not selected",
            ));
        }
        if let Some(build) = &document.build {
            diagnostics.extend(build.extra.keys().map(|property| {
                Diagnostic::warning(
                    format!("/build/{}", json_pointer_escape(property)),
                    "unknown-legacy-build-option",
                    format!(
                        "legacy build option {property:?} is allowed by the base schema and was preserved"
                    ),
                )
            }));
        }
    }

    diagnostics
}

fn validate_source(
    document: &super::model::RawDevContainer,
    source: SourceKind,
    issues: &mut Vec<ValidationIssue>,
) {
    if document.service.is_some() && source != SourceKind::Compose {
        issue(issues, "/service", "`service` requires `dockerComposeFile`");
    }
    if document.run_services.is_some() && source != SourceKind::Compose {
        issue(
            issues,
            "/runServices",
            "`runServices` requires `dockerComposeFile`",
        );
    }

    match source {
        SourceKind::Build => {
            validate_build_source(document, issues);
            validate_non_compose_shutdown(document, issues);
        }
        SourceKind::Compose => {
            if document.service.is_none() {
                issue(
                    issues,
                    "/service",
                    "Compose configurations require `service`",
                );
            }
            if document.workspace_folder.is_none() {
                issue(
                    issues,
                    "/workspaceFolder",
                    "Compose configurations require `workspaceFolder`",
                );
            }
            for (present, path, message) in [
                (
                    document.run_args.is_some(),
                    "/runArgs",
                    "`runArgs` is only valid for image or Dockerfile configurations",
                ),
                (
                    document.app_port.is_some(),
                    "/appPort",
                    "`appPort` is only valid for image or Dockerfile configurations",
                ),
                (
                    document.workspace_mount.is_some(),
                    "/workspaceMount",
                    "`workspaceMount` is only valid for image or Dockerfile configurations",
                ),
            ] {
                if present {
                    issue(issues, path, message);
                }
            }
            if document.shutdown_action == Some(ShutdownAction::StopContainer) {
                issue(
                    issues,
                    "/shutdownAction",
                    "Compose configurations accept only `none` or `stopCompose`",
                );
            }
        }
        SourceKind::Image => validate_non_compose_shutdown(document, issues),
        SourceKind::Unspecified => {
            for (present, path) in [
                (document.run_args.is_some(), "/runArgs"),
                (document.override_command.is_some(), "/overrideCommand"),
                (document.shutdown_action.is_some(), "/shutdownAction"),
                (document.workspace_folder.is_some(), "/workspaceFolder"),
                (document.workspace_mount.is_some(), "/workspaceMount"),
                (document.app_port.is_some(), "/appPort"),
            ] {
                if present {
                    issue(
                        issues,
                        path,
                        "property requires an image, Dockerfile, or Compose source",
                    );
                }
            }
        }
    }
}

fn validate_non_compose_shutdown(
    document: &super::model::RawDevContainer,
    issues: &mut Vec<ValidationIssue>,
) {
    if document.shutdown_action == Some(ShutdownAction::StopCompose) {
        issue(
            issues,
            "/shutdownAction",
            "image and Dockerfile configurations accept only `none` or `stopContainer`",
        );
    }
}

fn validate_build_source(
    document: &super::model::RawDevContainer,
    issues: &mut Vec<ValidationIssue>,
) {
    let nested_dockerfile = document
        .build
        .as_ref()
        .and_then(|build| build.dockerfile.as_ref());
    match (&document.docker_file, nested_dockerfile) {
        (None, None) => issue(
            issues,
            "/build/dockerfile",
            "Dockerfile configurations require `build.dockerfile` or legacy `dockerFile`",
        ),
        (Some(_), Some(_)) => issue(
            issues,
            "/",
            "`dockerFile` and `build.dockerfile` cannot be used together",
        ),
        (Some(_), None) => {}
        (None, Some(_)) => {
            if document.context.is_some() {
                issue(
                    issues,
                    "/context",
                    "current `build.dockerfile` syntax uses `build.context`",
                );
            }
        }
    }
}

fn validate_closed_objects(
    document: &super::model::RawDevContainer,
    issues: &mut Vec<ValidationIssue>,
) {
    if let Some(build) = &document.build
        && document.docker_file.is_none()
    {
        add_extra_issues(issues, "/build", &build.extra);
    }
    for (index, mount) in document.mounts.iter().enumerate() {
        if let RawMount::Object(mount) = mount {
            add_extra_issues(issues, &format!("/mounts/{index}"), &mount.extra);
        }
    }
    if let Some(attributes) = &document.other_ports_attributes {
        add_extra_issues(issues, "/otherPortsAttributes", &attributes.extra);
    }
    if let Some(requirements) = &document.host_requirements {
        add_extra_issues(issues, "/hostRequirements", &requirements.extra);
        if let Some(super::model::GpuRequirement::Detailed(gpu)) = &requirements.gpu {
            add_extra_issues(issues, "/hostRequirements/gpu", &gpu.extra);
        }
    }
    for (key, secret) in &document.secrets {
        add_extra_issues(
            issues,
            &format!("/secrets/{}", json_pointer_escape(key)),
            &secret.extra,
        );
    }
}

fn validate_ports(document: &super::model::RawDevContainer, issues: &mut Vec<ValidationIssue>) {
    for (index, port) in document.forward_ports.iter().enumerate() {
        if let Err(message) = normalize_forward_port(port) {
            issue(issues, format!("/forwardPorts/{index}"), message);
        }
    }
    for key in document.ports_attributes.keys() {
        if key.is_empty() {
            issue(
                issues,
                "/portsAttributes",
                "port attribute keys must not be empty",
            );
        }
    }
    if document
        .other_ports_attributes
        .as_ref()
        .and_then(|attributes| attributes.on_auto_forward)
        == Some(AutoForwardAction::OpenBrowserOnce)
    {
        issue(
            issues,
            "/otherPortsAttributes/onAutoForward",
            "`openBrowserOnce` is valid only in `portsAttributes`",
        );
    }
}

fn validate_host_requirements(
    document: &super::model::RawDevContainer,
    issues: &mut Vec<ValidationIssue>,
) {
    let Some(requirements) = &document.host_requirements else {
        return;
    };
    if requirements.cpus == Some(0) {
        issue(issues, "/hostRequirements/cpus", "must be at least 1");
    }
    for (path, value) in [
        ("/hostRequirements/memory", requirements.memory.as_deref()),
        ("/hostRequirements/storage", requirements.storage.as_deref()),
    ] {
        if value.is_some_and(|value| !valid_size(value)) {
            issue(
                issues,
                path,
                "must be decimal bytes or use a lowercase tb, gb, mb, or kb suffix",
            );
        }
    }
    match &requirements.gpu {
        Some(super::model::GpuRequirement::Name(value)) if value != "optional" => issue(
            issues,
            "/hostRequirements/gpu",
            "string GPU requirement must be `optional`",
        ),
        Some(super::model::GpuRequirement::Detailed(gpu)) => {
            if gpu.cores == Some(0) {
                issue(issues, "/hostRequirements/gpu/cores", "must be at least 1");
            }
            if gpu
                .memory
                .as_deref()
                .is_some_and(|value| !valid_size(value))
            {
                issue(
                    issues,
                    "/hostRequirements/gpu/memory",
                    "must be decimal bytes or use a lowercase tb, gb, mb, or kb suffix",
                );
            }
        }
        _ => {}
    }
}

fn validate_secrets(document: &super::model::RawDevContainer, issues: &mut Vec<ValidationIssue>) {
    for key in document.secrets.keys() {
        if !valid_environment_name(key) {
            issue(
                issues,
                format!("/secrets/{}", json_pointer_escape(key)),
                "secret name must match [A-Za-z_][A-Za-z0-9_]*",
            );
        }
    }
}

fn validate_mounts(document: &super::model::RawDevContainer, issues: &mut Vec<ValidationIssue>) {
    for (index, mount) in document.mounts.iter().enumerate() {
        let RawMount::Object(mount) = mount else {
            continue;
        };
        if mount.kind.is_none() {
            issue(
                issues,
                format!("/mounts/{index}/type"),
                "object mount requires `type`",
            );
        }
        if mount.target.is_none() {
            issue(
                issues,
                format!("/mounts/{index}/target"),
                "object mount requires `target`",
            );
        }
    }
}

fn normalize_source(
    document: &super::model::RawDevContainer,
    source: SourceKind,
) -> ContainerSource {
    match source {
        SourceKind::Image => document
            .image
            .clone()
            .map_or(ContainerSource::Unspecified, ContainerSource::Image),
        SourceKind::Build => {
            let build = document.build.clone().unwrap_or_default();
            let dockerfile = document
                .docker_file
                .clone()
                .or(build.dockerfile)
                .unwrap_or_default();
            let context = if document.docker_file.is_some() {
                document.context.clone().unwrap_or_else(|| ".".to_owned())
            } else {
                build.context.unwrap_or_else(|| ".".to_owned())
            };
            ContainerSource::Build(BuildConfig {
                dockerfile,
                context,
                target: build.target,
                args: build.args,
                cache_from: build
                    .cache_from
                    .map_or_else(Vec::new, super::model::OneOrMany::into_vec),
                options: build.options,
                extensions: build.extra,
            })
        }
        SourceKind::Compose => ContainerSource::Compose(ComposeConfig {
            files: document
                .docker_compose_file
                .clone()
                .map_or_else(Vec::new, super::model::OneOrMany::into_vec),
            service: document.service.clone().unwrap_or_default(),
            run_services: document.run_services.clone(),
        }),
        SourceKind::Unspecified => ContainerSource::Unspecified,
    }
}

fn normalize_mount(mount: RawMount) -> Option<Mount> {
    match mount {
        RawMount::String(value) => Some(Mount::String(value)),
        RawMount::Object(value) => Some(Mount::Object {
            kind: value.kind?,
            source: value.source,
            target: value.target?,
        }),
    }
}

fn normalize_forward_port(port: &RawForwardPort) -> Result<ForwardPort, &'static str> {
    match port {
        RawForwardPort::Number(value) => u16::try_from(*value)
            .map(ForwardPort::Number)
            .map_err(|_| "numeric port must be between 0 and 65535"),
        RawForwardPort::Host(value) => {
            let Some((host, port)) = value.rsplit_once(':') else {
                return Err("string port must have the form `host:port`");
            };
            if host.is_empty()
                || !host
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err("port host must contain lowercase letters, digits, or hyphens");
            }
            if port.is_empty() || port.len() > 5 || !port.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err("string port must contain one to five decimal digits");
            }
            let port = port
                .parse::<u32>()
                .map_err(|_| "string port must contain a decimal port")?;
            Ok(ForwardPort::Host {
                host: host.to_owned(),
                port,
            })
        }
    }
}

fn explicit_properties(document: &super::model::RawDevContainer) -> BTreeSet<&'static str> {
    let mut values = BTreeSet::new();
    for (present, name) in [
        (document.image.is_some(), "image"),
        (document.build.is_some(), "build"),
        (document.docker_file.is_some(), "dockerFile"),
        (document.context.is_some(), "context"),
        (document.docker_compose_file.is_some(), "dockerComposeFile"),
        (document.service.is_some(), "service"),
        (document.run_services.is_some(), "runServices"),
        (document.run_args.is_some(), "runArgs"),
        (document.override_command.is_some(), "overrideCommand"),
        (document.shutdown_action.is_some(), "shutdownAction"),
        (document.init.is_some(), "init"),
        (document.privileged.is_some(), "privileged"),
        (!document.cap_add.is_empty(), "capAdd"),
        (!document.security_opt.is_empty(), "securityOpt"),
        (document.workspace_folder.is_some(), "workspaceFolder"),
        (document.workspace_mount.is_some(), "workspaceMount"),
        (!document.mounts.is_empty(), "mounts"),
        (!document.container_env.is_empty(), "containerEnv"),
        (!document.remote_env.is_empty(), "remoteEnv"),
        (document.container_user.is_some(), "containerUser"),
        (document.remote_user.is_some(), "remoteUser"),
        (
            document.update_remote_user_uid.is_some(),
            "updateRemoteUserUID",
        ),
        (document.user_env_probe.is_some(), "userEnvProbe"),
        (!document.forward_ports.is_empty(), "forwardPorts"),
        (!document.ports_attributes.is_empty(), "portsAttributes"),
        (
            document.other_ports_attributes.is_some(),
            "otherPortsAttributes",
        ),
        (document.app_port.is_some(), "appPort"),
        (document.initialize_command.is_some(), "initializeCommand"),
        (document.on_create_command.is_some(), "onCreateCommand"),
        (
            document.update_content_command.is_some(),
            "updateContentCommand",
        ),
        (document.post_create_command.is_some(), "postCreateCommand"),
        (document.post_start_command.is_some(), "postStartCommand"),
        (document.post_attach_command.is_some(), "postAttachCommand"),
        (document.wait_for.is_some(), "waitFor"),
        (!document.features.is_empty(), "features"),
        (
            !document.override_feature_install_order.is_empty(),
            "overrideFeatureInstallOrder",
        ),
        (document.host_requirements.is_some(), "hostRequirements"),
        (!document.customizations.is_empty(), "customizations"),
        (!document.secrets.is_empty(), "secrets"),
    ] {
        if present {
            values.insert(name);
        }
    }
    values
}

fn add_extra_issues(
    issues: &mut Vec<ValidationIssue>,
    base: &str,
    extra: &indexmap::IndexMap<String, serde_json::Value>,
) {
    for property in extra.keys() {
        issue(
            issues,
            format!("{base}/{}", json_pointer_escape(property)),
            "property is not defined by the current Dev Container schema",
        );
    }
}

fn valid_size(value: &str) -> bool {
    let digit_count = value.bytes().take_while(u8::is_ascii_digit).count();
    digit_count > 0 && matches!(&value[digit_count..], "" | "tb" | "gb" | "mb" | "kb")
}

fn valid_environment_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn json_pointer_escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn issue(issues: &mut Vec<ValidationIssue>, path: impl Into<String>, message: impl Into<String>) {
    issues.push(ValidationIssue {
        path: path.into(),
        message: message.into(),
    });
}
