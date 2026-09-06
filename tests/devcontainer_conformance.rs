use std::path::PathBuf;

use jsonc_parser::{ParseOptions, parse_to_serde_value};
use jsonschema::{Draft, Validator};
use serde_json::{Map, Value, json};
use sshpod::devcontainer::{ConfigOrigin, parse_bytes};

const SCHEMA_TEXT: &str = include_str!("fixtures/devContainer.base.schema.json");
const JSONC_OPTIONS: ParseOptions = ParseOptions {
    allow_comments: true,
    allow_loose_object_property_names: false,
    allow_trailing_commas: false,
    allow_missing_commas: false,
    allow_single_quoted_strings: false,
    allow_hexadecimal_numbers: false,
    allow_unary_plus_numbers: false,
};

struct Case {
    name: String,
    document: Value,
    valid: bool,
}

impl Case {
    fn valid(name: impl Into<String>, document: Value) -> Self {
        Self {
            name: name.into(),
            document,
            valid: true,
        }
    }

    fn invalid(name: impl Into<String>, document: Value) -> Self {
        Self {
            name: name.into(),
            document,
            valid: false,
        }
    }
}

fn validator() -> Result<Validator, Box<dyn std::error::Error>> {
    let schema = serde_json::from_str(SCHEMA_TEXT)?;
    Ok(jsonschema::options()
        .with_draft(Draft::Draft201909)
        .build(&schema)?)
}

fn parser_accepts(name: &str, document: &Value) -> Result<bool, Box<dyn std::error::Error>> {
    let contents = serde_json::to_vec(document)?;
    let origin = ConfigOrigin::from_path(PathBuf::from(format!("{name}.json")), None);
    Ok(parse_bytes(origin, &contents).is_ok_and(|parsed| parsed.validate().is_ok()))
}

fn assert_cases(cases: Vec<Case>) -> Result<(), Box<dyn std::error::Error>> {
    let validator = validator()?;
    for case in cases {
        let schema_valid = validator.is_valid(&case.document);
        let schema_errors = validator
            .iter_errors(&case.document)
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        assert_eq!(
            schema_valid, case.valid,
            "pinned schema expectation failed for {}: {}",
            case.name, schema_errors
        );
        assert_eq!(
            parser_accepts(&case.name, &case.document)?,
            case.valid,
            "sshpod disagrees with the pinned schema for {}",
            case.name
        );
    }
    Ok(())
}

fn image_with(property: &str, value: Value) -> Value {
    let mut document = Map::from_iter([("image".to_owned(), json!("alpine"))]);
    document.insert(property.to_owned(), value);
    Value::Object(document)
}

#[test]
fn every_checked_in_valid_fixture_matches_the_pinned_schema()
-> Result<(), Box<dyn std::error::Error>> {
    let validator = validator()?;
    for name in [
        "minimal-image",
        "dockerfile",
        "compose",
        "jsonc-comments",
        "lifecycle-string",
        "lifecycle-array",
        "lifecycle-parallel",
        "mounts",
        "environment",
        "ports",
        "features",
        "variables",
        "all-properties",
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/devcontainers")
            .join(name)
            .join(".devcontainer/devcontainer.json");
        let contents = std::fs::read_to_string(&path)?;
        let document = parse_to_serde_value::<Value>(&contents, &JSONC_OPTIONS)?;
        let errors = validator
            .iter_errors(&document)
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            validator.is_valid(&document),
            "fixture {} does not satisfy the pinned schema: {errors}",
            path.display()
        );
        assert!(parser_accepts(name, &document)?, "sshpod rejected {name}");
    }
    Ok(())
}

#[test]
fn every_checked_in_semantic_invalid_fixture_is_rejected_by_both_validators()
-> Result<(), Box<dyn std::error::Error>> {
    let validator = validator()?;
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/devcontainers/invalid");
    for name in [
        "conflicting-sources.jsonc",
        "invalid-port.jsonc",
        "invalid-mount.jsonc",
    ] {
        let contents = std::fs::read_to_string(root.join(name))?;
        let document = parse_to_serde_value::<Value>(&contents, &JSONC_OPTIONS)?;
        assert!(!validator.is_valid(&document), "schema accepted {name}");
        assert!(!parser_accepts(name, &document)?, "sshpod accepted {name}");
    }
    Ok(())
}

#[test]
fn source_and_build_union_matrix_matches_the_schema() -> Result<(), Box<dyn std::error::Error>> {
    assert_cases(vec![
        Case::valid("image", json!({"image": "alpine"})),
        Case::valid(
            "nested-build",
            json!({"build": {"dockerfile": "Dockerfile"}}),
        ),
        Case::valid(
            "complete-build",
            json!({"build": {
                "dockerfile": "Dockerfile", "context": "..", "target": "dev",
                "args": {"VERSION": "1"}, "cacheFrom": ["one", "two"],
                "options": ["--pull"]
            }}),
        ),
        Case::valid("legacy-build", json!({"dockerFile": "Dockerfile"})),
        Case::valid(
            "legacy-build-options",
            json!({"dockerFile": "Dockerfile", "context": "..", "build": {
                "target": "dev", "cacheFrom": "base"
            }}),
        ),
        Case::valid(
            "compose-scalar",
            json!({"dockerComposeFile": "compose.yml", "service": "app", "workspaceFolder": "/work"}),
        ),
        Case::valid(
            "compose-array",
            json!({"dockerComposeFile": ["one.yml", "two.yml"], "service": "app", "workspaceFolder": "/work", "runServices": ["app", "db"]}),
        ),
        Case::invalid("root-array", json!([])),
        Case::invalid("image-wrong-type", json!({"image": 1})),
        Case::invalid(
            "source-conflict",
            json!({"image": "one", "build": {"dockerfile": "Dockerfile"}}),
        ),
        Case::invalid(
            "build-missing-dockerfile",
            json!({"build": {"target": "dev"}}),
        ),
        Case::invalid(
            "build-extra",
            json!({"build": {"dockerfile": "Dockerfile", "future": true}}),
        ),
        Case::invalid(
            "build-args-type",
            json!({"build": {"dockerfile": "Dockerfile", "args": {"A": 1}}}),
        ),
        Case::invalid(
            "build-cache-type",
            json!({"build": {"dockerfile": "Dockerfile", "cacheFrom": 1}}),
        ),
        Case::valid(
            "legacy-context-location",
            json!({"dockerFile": "Dockerfile", "build": {"context": ".."}}),
        ),
        Case::valid(
            "legacy-open-build-options",
            json!({"dockerFile": "Dockerfile", "build": {"future": true}}),
        ),
        Case::invalid(
            "current-context-location",
            json!({"context": "..", "build": {"dockerfile": "Dockerfile"}}),
        ),
        Case::invalid(
            "compose-missing-service",
            json!({"dockerComposeFile": "compose.yml", "workspaceFolder": "/work"}),
        ),
        Case::invalid(
            "compose-missing-workspace",
            json!({"dockerComposeFile": "compose.yml", "service": "app"}),
        ),
        Case::invalid(
            "compose-with-run-args",
            json!({"dockerComposeFile": "compose.yml", "service": "app", "workspaceFolder": "/work", "runArgs": []}),
        ),
        Case::invalid(
            "compose-file-type",
            json!({"dockerComposeFile": 1, "service": "app", "workspaceFolder": "/work"}),
        ),
        Case::invalid(
            "compose-file-item-type",
            json!({"dockerComposeFile": ["one.yml", 2], "service": "app", "workspaceFolder": "/work"}),
        ),
        Case::invalid(
            "compose-service-type",
            json!({"dockerComposeFile": "compose.yml", "service": true, "workspaceFolder": "/work"}),
        ),
        Case::invalid(
            "image-with-service",
            json!({"image": "alpine", "service": "app"}),
        ),
        Case::invalid("common-with-workspace", json!({"workspaceFolder": "/work"})),
    ])
}

#[test]
fn lifecycle_union_and_enum_matrix_matches_the_schema() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = Vec::new();
    for property in [
        "initializeCommand",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ] {
        cases.push(Case::valid(
            format!("{property}-shell"),
            image_with(property, json!("echo ready")),
        ));
        cases.push(Case::valid(
            format!("{property}-exec"),
            image_with(property, json!(["echo", "ready"])),
        ));
        cases.push(Case::valid(
            format!("{property}-parallel"),
            image_with(property, json!({"one": "true", "two": ["true"]})),
        ));
        cases.push(Case::invalid(
            format!("{property}-boolean"),
            image_with(property, json!(true)),
        ));
        cases.push(Case::invalid(
            format!("{property}-invalid-exec-item"),
            image_with(property, json!(["echo", 1])),
        ));
        cases.push(Case::invalid(
            format!("{property}-invalid-parallel-value"),
            image_with(property, json!({"one": true})),
        ));
    }
    for value in [
        "initializeCommand",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
    ] {
        cases.push(Case::valid(
            format!("waitFor-{value}"),
            image_with("waitFor", json!(value)),
        ));
    }
    cases.push(Case::invalid(
        "waitFor-invalid",
        image_with("waitFor", json!("postAttachCommand")),
    ));
    assert_cases(cases)
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the port, mount, and host schema matrix is clearer as one declarative table"
)]
fn ports_mounts_and_host_requirements_match_the_schema() -> Result<(), Box<dyn std::error::Error>> {
    assert_cases(vec![
        Case::valid("forward-port-zero", image_with("forwardPorts", json!([0]))),
        Case::valid(
            "forward-port-max",
            image_with("forwardPorts", json!([65535])),
        ),
        Case::valid(
            "forward-port-host",
            image_with("forwardPorts", json!(["db:99999"])),
        ),
        Case::invalid(
            "forward-port-negative",
            image_with("forwardPorts", json!([-1])),
        ),
        Case::invalid(
            "forward-port-overflow",
            image_with("forwardPorts", json!([65536])),
        ),
        Case::invalid(
            "forward-port-host-case",
            image_with("forwardPorts", json!(["DB:5432"])),
        ),
        Case::invalid(
            "forward-port-six-digits",
            image_with("forwardPorts", json!(["db:100000"])),
        ),
        Case::valid(
            "app-port-unions",
            image_with("appPort", json!([8080, "8000:8010"])),
        ),
        Case::valid("app-port-scalar-number", image_with("appPort", json!(8080))),
        Case::valid(
            "app-port-scalar-string",
            image_with("appPort", json!("8080:80")),
        ),
        Case::invalid(
            "app-port-object",
            image_with("appPort", json!({"port": 8080})),
        ),
        Case::valid(
            "mount-string",
            image_with("mounts", json!(["anything accepted by Docker later"])),
        ),
        Case::valid(
            "mount-bind",
            image_with("mounts", json!([{"type": "bind", "target": "/data"}])),
        ),
        Case::valid(
            "mount-volume",
            image_with(
                "mounts",
                json!([{"type": "volume", "source": "cache", "target": "/cache"}]),
            ),
        ),
        Case::invalid(
            "mount-missing-type",
            image_with("mounts", json!([{"target": "/data"}])),
        ),
        Case::invalid(
            "mount-missing-target",
            image_with("mounts", json!([{"type": "bind"}])),
        ),
        Case::invalid(
            "mount-invalid-type",
            image_with("mounts", json!([{"type": "tmpfs", "target": "/data"}])),
        ),
        Case::invalid(
            "mount-extra",
            image_with(
                "mounts",
                json!([{"type": "bind", "target": "/data", "readonly": true}]),
            ),
        ),
        Case::valid(
            "host-minimums",
            image_with(
                "hostRequirements",
                json!({"cpus": 1, "memory": "1", "storage": "1kb"}),
            ),
        ),
        Case::valid(
            "gpu-values",
            image_with(
                "hostRequirements",
                json!({"gpu": {"cores": 1, "memory": "1tb"}}),
            ),
        ),
        Case::valid(
            "gpu-optional",
            image_with("hostRequirements", json!({"gpu": "optional"})),
        ),
        Case::valid(
            "gpu-booleans",
            image_with("hostRequirements", json!({"gpu": true})),
        ),
        Case::invalid(
            "host-zero-cpus",
            image_with("hostRequirements", json!({"cpus": 0})),
        ),
        Case::invalid(
            "host-size-case",
            image_with("hostRequirements", json!({"memory": "1GB"})),
        ),
        Case::invalid(
            "host-size-unit",
            image_with("hostRequirements", json!({"storage": "1k"})),
        ),
        Case::invalid(
            "gpu-name",
            image_with("hostRequirements", json!({"gpu": "required"})),
        ),
        Case::invalid(
            "gpu-zero-cores",
            image_with("hostRequirements", json!({"gpu": {"cores": 0}})),
        ),
        Case::invalid(
            "host-extra",
            image_with("hostRequirements", json!({"cpuArchitecture": "amd64"})),
        ),
    ])
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the remaining base-schema property matrix is clearer as one declarative table"
)]
fn remaining_types_and_closed_objects_match_the_schema() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = vec![
        Case::valid(
            "remote-env-null",
            image_with("remoteEnv", json!({"REMOVE": null})),
        ),
        Case::valid(
            "features-arbitrary-options",
            image_with(
                "features",
                json!({"example:1": {"flag": true}, "legacy": "yes"}),
            ),
        ),
        Case::valid(
            "secret",
            image_with(
                "secrets",
                json!({"TOKEN": {"description": "token", "documentationUrl": "https://example.com"}}),
            ),
        ),
        Case::valid(
            "additional-properties-metadata",
            image_with("additionalProperties", json!({"future": [1, 2]})),
        ),
        Case::invalid("name-type", image_with("name", json!(1))),
        Case::invalid(
            "workspace-folder-type",
            image_with("workspaceFolder", json!(true)),
        ),
        Case::invalid(
            "workspace-mount-type",
            image_with("workspaceMount", json!({"type": "bind"})),
        ),
        Case::invalid("features-type", image_with("features", json!([]))),
        Case::invalid(
            "feature-order-type",
            image_with("overrideFeatureInstallOrder", json!("feature")),
        ),
        Case::invalid(
            "secret-name",
            image_with("secrets", json!({"INVALID-NAME": {}})),
        ),
        Case::invalid(
            "secret-extra",
            image_with("secrets", json!({"TOKEN": {"required": true}})),
        ),
        Case::invalid(
            "container-env-value",
            image_with("containerEnv", json!({"A": true})),
        ),
        Case::invalid(
            "remote-env-value",
            image_with("remoteEnv", json!({"A": true})),
        ),
        Case::invalid(
            "customizations-type",
            image_with("customizations", json!([])),
        ),
        Case::invalid(
            "additional-properties-type",
            image_with("additionalProperties", json!([])),
        ),
        Case::invalid("run-args-type", image_with("runArgs", json!("--init"))),
        Case::invalid("run-args-item", image_with("runArgs", json!([1]))),
        Case::invalid("container-user-type", image_with("containerUser", json!(1))),
        Case::invalid("remote-user-type", image_with("remoteUser", json!(1))),
        Case::invalid("uid-type", image_with("updateRemoteUserUID", json!("true"))),
    ];
    for property in ["init", "privileged", "overrideCommand"] {
        cases.push(Case::valid(
            format!("{property}-boolean"),
            image_with(property, json!(true)),
        ));
        cases.push(Case::invalid(
            format!("{property}-type"),
            image_with(property, json!("true")),
        ));
    }
    for property in ["capAdd", "securityOpt"] {
        cases.push(Case::valid(
            format!("{property}-array"),
            image_with(property, json!(["VALUE"])),
        ));
        cases.push(Case::invalid(
            format!("{property}-type"),
            image_with(property, json!("VALUE")),
        ));
    }
    for value in [
        "none",
        "loginShell",
        "loginInteractiveShell",
        "interactiveShell",
    ] {
        cases.push(Case::valid(
            format!("userEnvProbe-{value}"),
            image_with("userEnvProbe", json!(value)),
        ));
    }
    cases.push(Case::invalid(
        "userEnvProbe-invalid",
        image_with("userEnvProbe", json!("shell")),
    ));
    assert_cases(cases)
}

#[test]
fn port_attribute_enum_and_closed_object_matrix_matches_the_schema()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cases = Vec::new();
    for value in [
        "notify",
        "openBrowser",
        "openBrowserOnce",
        "openPreview",
        "silent",
        "ignore",
    ] {
        cases.push(Case::valid(
            format!("port-action-{value}"),
            image_with("portsAttributes", json!({"3000": {"onAutoForward": value}})),
        ));
    }
    for value in ["notify", "openBrowser", "openPreview", "silent", "ignore"] {
        cases.push(Case::valid(
            format!("other-port-action-{value}"),
            image_with("otherPortsAttributes", json!({"onAutoForward": value})),
        ));
    }
    for value in ["http", "https"] {
        cases.push(Case::valid(
            format!("port-protocol-{value}"),
            image_with("portsAttributes", json!({"3000": {"protocol": value}})),
        ));
    }
    cases.extend([
        Case::valid(
            "complete-port-attributes",
            image_with(
                "portsAttributes",
                json!({"3000": {
                    "label": "Web", "requireLocalPort": true, "elevateIfNeeded": false
                }}),
            ),
        ),
        Case::invalid(
            "empty-port-key",
            image_with("portsAttributes", json!({"": {}})),
        ),
        Case::invalid(
            "invalid-port-action",
            image_with(
                "portsAttributes",
                json!({"3000": {"onAutoForward": "open"}}),
            ),
        ),
        Case::invalid(
            "other-open-once",
            image_with(
                "otherPortsAttributes",
                json!({"onAutoForward": "openBrowserOnce"}),
            ),
        ),
        Case::invalid(
            "invalid-port-protocol",
            image_with("portsAttributes", json!({"3000": {"protocol": "tcp"}})),
        ),
        Case::valid(
            "port-attributes-extra",
            image_with("portsAttributes", json!({"3000": {"host": "localhost"}})),
        ),
        Case::invalid(
            "ports-attributes-type",
            image_with("portsAttributes", json!([])),
        ),
        Case::invalid(
            "other-ports-attributes-extra",
            image_with("otherPortsAttributes", json!({"host": "localhost"})),
        ),
    ]);
    assert_cases(cases)
}

#[test]
fn documented_reference_compatibility_divergences_are_deliberate_and_tested()
-> Result<(), Box<dyn std::error::Error>> {
    let validator = validator()?;
    for (name, document) in [
        (
            "future-property",
            json!({"image": "alpine", "futureProperty": {"enabled": true}}),
        ),
        // The official CLI's DevContainerFromImageConfig permits a missing
        // image when configuring an existing container. The schema's second
        // oneOf branch intends to describe this case, but its $ref sibling
        // `additionalProperties: false` rejects common properties under draft
        // 2019-09. Keep reference-implementation compatibility explicitly.
        ("existing-container-metadata", json!({"name": "metadata"})),
    ] {
        assert!(!validator.is_valid(&document));
        let contents = serde_json::to_vec(&document)?;
        let parsed = parse_bytes(
            ConfigOrigin::from_path(PathBuf::from(format!("{name}.json")), None),
            &contents,
        )?;
        let normalized = parsed.validate()?;
        if name == "future-property" {
            assert!(
                normalized
                    .metadata
                    .extensions
                    .contains_key("futureProperty")
            );
            assert!(
                normalized
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "unknown-property")
            );
        }
    }
    Ok(())
}
