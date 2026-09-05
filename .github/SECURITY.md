# Security policy

## Supported versions

sshpod is early-stage software. Security fixes target the latest 0.1.x version
and the current development branch. Please reproduce reports with the newest
available code when practical.

## Reporting a vulnerability

Do not post suspected vulnerabilities in public issues or pull requests.
Email [nbari@tequila.io](mailto:nbari@tequila.io) privately with:

- The affected sshpod version (`sshpod --version`), OS, and Podman version
- A description of the impact and steps to reproduce it
- A minimal proof of concept, with credentials and private host details removed
- Any suggested mitigation or fix

The maintainer will assess the report and coordinate a fix and disclosure
timeline. Please keep details private until a fix is available or a disclosure
date has been agreed upon.

## Scope

Issues in sshpod's command execution, argument handling, output handling, and use
of dependencies are in scope. Version 0.1.x only checks the local Podman CLI;
it does not yet create workspaces or manage remote hosts.

Podman and SSH provide their own runtime and transport security. Report defects
confined to those projects upstream; report unsafe use of them by sshpod here.
Never include SSH private keys, tokens, or other secrets in a report.
