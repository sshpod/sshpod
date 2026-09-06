# Dev Container parser fixtures

The offline conformance oracle is
[`../devContainer.base.schema.json`](../devContainer.base.schema.json), copied
verbatim from `devcontainers/spec` commit
`c95ffeed1d059abfe9ffbe79762dc2fa4e7c2421`. Its immutable URL, SHA-256, and
license are recorded in
[`../devContainer.base.schema.source`](../devContainer.base.schema.source).

`tests/devcontainer_conformance.rs` compiles that schema as JSON Schema draft
2019-09 and compares its result with sshpod for:

- every valid and semantic-invalid fixture;
- every container-source representation;
- scalar/array/object unions;
- lifecycle command representations and enum values;
- port boundaries and attributes;
- mount representations;
- host requirement boundaries;
- known-property type failures and closed objects.

The suite separately asserts two deliberate compatibility exceptions instead of
hiding them:

1. Unknown top-level fields are accepted, preserved, and warned about, as
   required for sshpod's forward-compatibility policy.
2. Common metadata without a source is accepted for existing-container flows.
   The reference CLI permits this (`image` is optional for an existing
   container), while the base schema's intended metadata-only branch is rejected
   by a strict draft 2019-09 validator because it combines `$ref` with sibling
   `additionalProperties: false`.

Reference behavior was inspected at Dev Container CLI commit
`5dc7533314b5ba7ec3875c30143dfe1aec644870`, particularly
`src/spec-configuration/configuration.ts` and
`src/spec-common/injectHeadless.ts`. The CLI is not executed in ordinary tests:
it has no stable, side-effect-free schema-validation command, and container-based
differential tests would make this parser suite depend on Docker/Podman, Node, and
the network.

Run `just devcontainer-schema-check` to compare the pinned schema with upstream
`main`. If it changes, review the diff first, then update the model, validation,
normalization, cases, pinned file, commit metadata, and SHA-256 together.
