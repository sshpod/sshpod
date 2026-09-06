use std::{fs, path::PathBuf, process};

use sshpod::devcontainer::{
    self, AppPort, AutoForwardAction, ConfigOrigin, ContainerSource, ForwardPort, LifecycleCommand,
    LifecycleCommandValue, Mount, ShutdownAction, UserEnvProbe, WaitFor,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/devcontainers")
        .join(name)
        .join(".devcontainer/devcontainer.json")
}

fn load_fixture(
    name: &str,
) -> Result<devcontainer::NormalizedDevContainer, Box<dyn std::error::Error>> {
    Ok(devcontainer::load(fixture(name))?)
}

fn parse_inline(
    name: &str,
    contents: &[u8],
) -> Result<devcontainer::NormalizedDevContainer, Box<dyn std::error::Error>> {
    Ok(
        devcontainer::parse_bytes(ConfigOrigin::from_path(PathBuf::from(name), None), contents)?
            .validate()?,
    )
}

#[test]
fn parses_and_normalizes_image_build_and_compose_sources() -> Result<(), Box<dyn std::error::Error>>
{
    let image = load_fixture("minimal-image")?;
    assert_eq!(
        image.source,
        ContainerSource::Image("alpine:latest".to_owned())
    );
    assert_eq!(image.origin.config_path, fixture("minimal-image"));

    let build = load_fixture("dockerfile")?;
    let ContainerSource::Build(build) = build.source else {
        return Err("expected a build source".into());
    };
    assert_eq!(build.dockerfile, "Dockerfile");
    assert_eq!(build.context, "..");
    assert_eq!(build.target.as_deref(), Some("development"));
    assert_eq!(
        build.args.get("RUST_VERSION").map(String::as_str),
        Some("stable")
    );
    assert_eq!(build.cache_from, ["rust:latest"]);
    assert_eq!(build.options, ["--pull"]);

    let compose = load_fixture("compose")?;
    let ContainerSource::Compose(compose) = compose.source else {
        return Err("expected a Compose source".into());
    };
    assert_eq!(compose.files, ["compose.yml", "compose.dev.yml"]);
    assert_eq!(compose.service, "app");
    assert_eq!(
        compose.run_services,
        Some(vec!["app".to_owned(), "database".to_owned()])
    );
    Ok(())
}

#[test]
fn models_scalar_unions_legacy_builds_and_metadata_only_documents()
-> Result<(), Box<dyn std::error::Error>> {
    let legacy = parse_inline(
        "legacy.jsonc",
        br#"{
            "dockerFile":"Dockerfile",
            "context":"..",
            "build":{"cacheFrom":"base:latest"}
        }"#,
    )?;
    let ContainerSource::Build(build) = legacy.source else {
        return Err("expected a legacy build source".into());
    };
    assert_eq!(build.cache_from, ["base:latest"]);
    assert!(
        legacy
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "legacy-dockerfile")
    );

    let compose = parse_inline(
        "compose.jsonc",
        br#"{"dockerComposeFile":"compose.yml","service":"app","workspaceFolder":"/work"}"#,
    )?;
    assert!(matches!(
        compose.source,
        ContainerSource::Compose(ref source) if source.files == ["compose.yml"]
    ));

    let metadata = parse_inline(
        "metadata.jsonc",
        br#"{"customizations":{"example":{"enabled":true}}}"#,
    )?;
    assert_eq!(metadata.source, ContainerSource::Unspecified);
    Ok(())
}

#[test]
fn applies_only_context_independent_spec_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let image = load_fixture("minimal-image")?;
    assert_eq!(image.runtime.override_command, Some(true));
    assert_eq!(
        image.runtime.shutdown_action,
        Some(ShutdownAction::StopContainer)
    );
    assert_eq!(image.lifecycle.wait_for, WaitFor::UpdateContent);
    assert_eq!(
        image.environment.user_env_probe,
        UserEnvProbe::LoginInteractiveShell
    );
    assert_eq!(image.environment.update_remote_user_uid, None);

    let compose = load_fixture("compose")?;
    assert_eq!(compose.runtime.override_command, Some(false));
    assert_eq!(
        compose.runtime.shutdown_action,
        Some(ShutdownAction::StopCompose)
    );

    let build = parse_inline(
        "default-context.jsonc",
        br#"{"build":{"dockerfile":"Dockerfile"}}"#,
    )?;
    assert!(matches!(
        build.source,
        ContainerSource::Build(ref source) if source.context == "."
    ));
    Ok(())
}

#[test]
fn parses_jsonc_without_modifying_comment_like_strings() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_fixture("jsonc-comments")?;
    assert_eq!(
        config.source,
        ContainerSource::Image("rust:latest".to_owned())
    );
    assert_eq!(
        config.environment.container.get("URL").map(String::as_str),
        Some("https://example.com/path/*literal*/")
    );
    Ok(())
}

#[test]
fn models_every_lifecycle_command_representation() -> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(
        load_fixture("lifecycle-string")?.lifecycle.post_create,
        Some(LifecycleCommand::Shell(ref command)) if command == "cargo build"
    ));
    assert!(matches!(
        load_fixture("lifecycle-array")?.lifecycle.post_create,
        Some(LifecycleCommand::Exec(ref command)) if command == &["cargo", "build"]
    ));

    let parallel = load_fixture("lifecycle-parallel")?;
    let Some(LifecycleCommand::Parallel(commands)) = parallel.lifecycle.post_create else {
        return Err("expected parallel lifecycle commands".into());
    };
    assert!(matches!(
        commands.get("dependencies"),
        Some(LifecycleCommandValue::Shell(command)) if command == "cargo fetch"
    ));
    assert!(matches!(
        commands.get("build"),
        Some(LifecycleCommandValue::Exec(command)) if command == &["cargo", "build"]
    ));
    Ok(())
}

#[test]
fn normalizes_string_and_object_mounts() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_fixture("mounts")?;
    assert_eq!(config.workspace.mounts.len(), 2);
    assert!(matches!(
        config.workspace.mounts.first(),
        Some(Mount::String(_))
    ));
    assert!(matches!(
        config.workspace.mounts.get(1),
        Some(Mount::Object { source: Some(source), target, .. })
            if source == "${localWorkspaceFolder}/data" && target == "/data"
    ));
    Ok(())
}

#[test]
fn parses_environment_users_and_preserves_variables() -> Result<(), Box<dyn std::error::Error>> {
    let environment = load_fixture("environment")?;
    assert_eq!(environment.environment.remote.get("REMOVED"), Some(&None));
    assert_eq!(
        environment.environment.user_env_probe,
        UserEnvProbe::LoginShell
    );
    assert_eq!(environment.environment.update_remote_user_uid, Some(false));

    let variables = load_fixture("variables")?;
    assert_eq!(
        variables.source,
        ContainerSource::Image("${localEnv:DEVCONTAINER_IMAGE:alpine:latest}".to_owned())
    );
    assert_eq!(
        variables
            .environment
            .container
            .get("HOME_FROM_HOST")
            .map(String::as_str),
        Some("${localEnv:HOME}")
    );
    assert_eq!(
        variables.workspace.folder.as_deref(),
        Some("${containerWorkspaceFolder}")
    );
    Ok(())
}

#[test]
fn parses_numeric_host_ports_and_port_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_fixture("ports")?;
    assert_eq!(
        config.ports.forward,
        [
            ForwardPort::Number(3000),
            ForwardPort::Host {
                host: "database".to_owned(),
                port: 5432
            }
        ]
    );
    assert!(matches!(
        config.ports.app.first(),
        Some(AppPort::Number(port)) if port.as_u64() == Some(8080)
    ));
    assert_eq!(
        config
            .ports
            .attributes
            .get("3000")
            .and_then(|attributes| attributes.on_auto_forward),
        Some(AutoForwardAction::OpenBrowserOnce)
    );
    Ok(())
}

#[test]
fn preserves_feature_order_and_options() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_fixture("features")?;
    assert_eq!(config.features.declarations.len(), 2);
    assert_eq!(
        config
            .features
            .declarations
            .first()
            .map(|feature| feature.reference.as_str()),
        Some("ghcr.io/devcontainers/features/rust:1")
    );
    let first = config
        .features
        .declarations
        .first()
        .ok_or("expected the first Feature")?;
    assert_eq!(
        first.options.get("version"),
        Some(&serde_json::json!("stable"))
    );
    assert_eq!(
        config.features.override_install_order,
        [
            "ghcr.io/devcontainers/features/github-cli",
            "ghcr.io/devcontainers/features/rust"
        ]
    );
    Ok(())
}

#[test]
fn parses_all_current_base_schema_categories() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_fixture("all-properties")?;
    assert_eq!(
        config.metadata.name.as_deref(),
        Some("Complete parser fixture")
    );
    assert_eq!(config.lifecycle.wait_for, WaitFor::PostCreate);
    assert_eq!(
        config
            .host_requirements
            .as_ref()
            .and_then(|requirements| requirements.cpus),
        Some(2)
    );
    assert!(config.metadata.customizations.contains_key("vscode"));
    assert!(config.metadata.secrets.contains_key("TOKEN"));
    assert!(config.metadata.additional_properties.is_some());
    Ok(())
}

#[test]
fn separates_parse_errors_from_aggregate_validation_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let invalid_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/devcontainers/invalid");
    let malformed = devcontainer::parse(invalid_root.join("malformed.jsonc"));
    let message = malformed
        .err()
        .ok_or("expected malformed JSONC to fail")?
        .to_string();
    assert!(message.contains("malformed.jsonc"));
    assert!(message.contains("line"));
    assert!(message.contains("column"));

    let parsed = devcontainer::parse(invalid_root.join("conflicting-sources.jsonc"))?;
    let validation = parsed.validate();
    let message = validation
        .err()
        .ok_or("expected validation to fail")?
        .to_string();
    assert!(message.contains("mutually exclusive"));

    for fixture in ["invalid-port.jsonc", "invalid-mount.jsonc"] {
        let parsed = devcontainer::parse(invalid_root.join(fixture))?;
        assert!(
            parsed.validate().is_err(),
            "{fixture} should fail validation"
        );
    }
    Ok(())
}

#[test]
fn rejects_trailing_commas_and_malformed_known_property_types() {
    let trailing = devcontainer::parse_bytes(
        ConfigOrigin::from_path(PathBuf::from("trailing.jsonc"), None),
        br#"{"image":"alpine",}"#,
    );
    assert!(trailing.is_err());

    let invalid_type = devcontainer::parse_bytes(
        ConfigOrigin::from_path(PathBuf::from("type.jsonc"), None),
        br#"{"image":"alpine","forwardPorts":"3000"}"#,
    );
    let message = invalid_type
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("expected a sequence"));
    assert!(message.contains("line"));
    assert!(message.contains("column"));
}

#[test]
fn preserves_unknown_top_level_properties_with_a_warning() -> Result<(), Box<dyn std::error::Error>>
{
    let parsed = devcontainer::parse_bytes(
        ConfigOrigin::from_path(PathBuf::from("future.jsonc"), None),
        br#"{"image":"alpine","futureProperty":{"enabled":true}}"#,
    )?;
    let config = parsed.validate()?;
    assert!(config.metadata.extensions.contains_key("futureProperty"));
    assert_eq!(config.diagnostics.len(), 1);
    assert_eq!(
        config
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.path.as_str()),
        Some("/futureProperty")
    );
    Ok(())
}

#[test]
fn discovery_returns_every_supported_candidate() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("sshpod-discovery-{}", process::id()));
    let primary = root.join(".devcontainer/devcontainer.json");
    let root_config = root.join(".devcontainer.json");
    let nested = root.join(".devcontainer/rust/devcontainer.json");
    for path in [&primary, &root_config, &nested] {
        let parent = path.parent().ok_or("test path has no parent")?;
        fs::create_dir_all(parent)?;
        fs::write(path, r#"{"image":"alpine"}"#)?;
    }
    let candidates = devcontainer::discover(&root)?;
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.relative_path.as_str())
            .collect::<Vec<_>>(),
        [
            ".devcontainer/devcontainer.json",
            ".devcontainer.json",
            ".devcontainer/rust/devcontainer.json"
        ]
    );
    fs::remove_dir_all(root)?;
    Ok(())
}
