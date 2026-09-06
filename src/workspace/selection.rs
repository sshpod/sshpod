use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result, bail, ensure};

use crate::devcontainer::{PRIMARY_CONFIG, ROOT_CONFIG};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Selection {
    NonInteractive,
    Index(usize),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ConfigChoice {
    pub(crate) path: String,
    pub(crate) persist: bool,
}

pub(crate) fn choose_config(
    source: &str,
    candidates: &[String],
    requested: Option<&str>,
    persisted: Option<&str>,
) -> Result<ConfigChoice> {
    if requested.is_some()
        || persisted.is_some()
        || candidates.len() <= 1
        || candidates
            .first()
            .is_some_and(|path| is_standard_path(path))
        || !io::stdin().is_terminal()
    {
        return select_config(
            source,
            candidates,
            requested,
            persisted,
            Selection::NonInteractive,
        );
    }

    let mut stdout = io::stdout().lock();
    writeln!(
        stdout,
        "Multiple Dev Container configurations found in {source:?}:\n"
    )?;
    for (index, path) in candidates.iter().enumerate() {
        writeln!(stdout, "  {}. {path}", index + 1)?;
    }
    write!(stdout, "\nSelect configuration [1/{}]: ", candidates.len())?;
    stdout.flush()?;
    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .context("failed to read Dev Container configuration selection")?;
    let selected = response
        .trim()
        .parse::<usize>()
        .context("Dev Container configuration selection must be a number")?;
    select_config(source, candidates, None, None, Selection::Index(selected))
}

pub(crate) fn select_config(
    source: &str,
    candidates: &[String],
    requested: Option<&str>,
    persisted: Option<&str>,
    selection: Selection,
) -> Result<ConfigChoice> {
    ensure!(
        !candidates.is_empty(),
        "no Dev Container configuration found in workspace source {source:?}"
    );
    if let Some(requested) = requested {
        let path = candidates
            .iter()
            .find(|candidate| candidate.as_str() == requested)
            .with_context(|| selection_error("requested", requested, source, candidates))?;
        return Ok(ConfigChoice {
            path: path.clone(),
            persist: true,
        });
    }
    if let Some(persisted) = persisted {
        let path = candidates
            .iter()
            .find(|candidate| candidate.as_str() == persisted)
            .with_context(|| selection_error("saved", persisted, source, candidates))?;
        return Ok(ConfigChoice {
            path: path.clone(),
            persist: true,
        });
    }
    if let Some(path) = candidates.first()
        && (is_standard_path(path) || candidates.len() == 1)
    {
        return Ok(ConfigChoice {
            path: path.clone(),
            persist: false,
        });
    }
    match selection {
        Selection::NonInteractive => bail!(
            "multiple Dev Container configurations found in {source:?}; use --config <path> to select one (available: {})",
            candidates.join(", ")
        ),
        Selection::Index(index) => {
            ensure!(
                index > 0,
                "Dev Container configuration selection must be between 1 and {}",
                candidates.len()
            );
            let path = candidates.get(index - 1).with_context(|| {
                format!(
                    "Dev Container configuration selection must be between 1 and {}",
                    candidates.len()
                )
            })?;
            Ok(ConfigChoice {
                path: path.clone(),
                persist: true,
            })
        }
    }
}

fn is_standard_path(path: &str) -> bool {
    matches!(path, PRIMARY_CONFIG | ROOT_CONFIG)
}

fn selection_error(kind: &str, path: &str, source: &str, candidates: &[String]) -> String {
    format!(
        "{kind} Dev Container configuration {path:?} was not found in workspace source {source:?}; use --config <path> to select one (available: {})",
        candidates.join(", ")
    )
}

pub(crate) fn choose_provider(
    workspace: &str,
    candidates: &[String],
    requested: Option<&str>,
) -> Result<String> {
    if requested.is_some() || candidates.len() <= 1 {
        return select_provider(workspace, candidates, requested, Selection::NonInteractive);
    }
    if !io::stdin().is_terminal() {
        return select_provider(workspace, candidates, None, Selection::NonInteractive);
    }

    let mut stdout = io::stdout().lock();
    writeln!(stdout, "Multiple providers available for {workspace:?}:\n")?;
    for (index, provider) in candidates.iter().enumerate() {
        writeln!(stdout, "  {}. {provider}", index + 1)?;
    }
    write!(stdout, "\nSelect provider [1/{}]: ", candidates.len())?;
    stdout.flush()?;
    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .context("failed to read provider selection")?;
    let selected = response
        .trim()
        .parse::<usize>()
        .context("provider selection must be a number")?;
    select_provider(workspace, candidates, None, Selection::Index(selected))
}

pub(crate) fn select_provider(
    workspace: &str,
    candidates: &[String],
    requested: Option<&str>,
    selection: Selection,
) -> Result<String> {
    ensure!(
        !candidates.is_empty(),
        "workspace {workspace:?} has no providers"
    );
    if let Some(requested) = requested {
        ensure!(
            candidates.iter().any(|candidate| candidate == requested),
            "workspace {workspace:?} is not configured for provider {requested:?}"
        );
        return Ok(requested.to_owned());
    }
    if let [provider] = candidates {
        return Ok(provider.clone());
    }
    match selection {
        Selection::NonInteractive => {
            bail!("workspace {workspace:?} has multiple providers; use --provider <name>")
        }
        Selection::Index(index) => {
            ensure!(
                index > 0,
                "provider selection must be between 1 and {}",
                candidates.len()
            );
            candidates.get(index - 1).cloned().with_context(|| {
                format!(
                    "provider selection must be between 1 and {}",
                    candidates.len()
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Selection, select_config, select_provider};

    fn candidates() -> Vec<String> {
        vec!["local".to_owned(), "sandbox".to_owned()]
    }

    #[test]
    fn selects_only_or_requested_provider() -> anyhow::Result<()> {
        assert_eq!(
            select_provider(
                "demo",
                &["local".to_owned()],
                None,
                Selection::NonInteractive
            )?,
            "local"
        );
        assert_eq!(
            select_provider(
                "demo",
                &candidates(),
                Some("sandbox"),
                Selection::NonInteractive
            )?,
            "sandbox"
        );
        Ok(())
    }

    #[test]
    fn non_interactive_ambiguity_requires_provider() {
        let error = select_provider("demo", &candidates(), None, Selection::NonInteractive);
        assert!(error.is_err());
        assert!(
            error
                .err()
                .is_some_and(|error| error.to_string().contains("--provider"))
        );
    }

    #[test]
    fn accepts_interactive_index_and_rejects_invalid_index() -> anyhow::Result<()> {
        assert_eq!(
            select_provider("demo", &candidates(), None, Selection::Index(2))?,
            "sandbox"
        );
        assert!(select_provider("demo", &candidates(), None, Selection::Index(0)).is_err());
        assert!(select_provider("demo", &candidates(), None, Selection::Index(3)).is_err());
        Ok(())
    }

    #[test]
    fn selects_requested_and_nested_devcontainer_configs() -> anyhow::Result<()> {
        let configs = vec![
            ".devcontainer/go/devcontainer.json".to_owned(),
            ".devcontainer/rust/devcontainer.json".to_owned(),
        ];
        let requested = select_config(
            "/workspace",
            &configs,
            Some(".devcontainer/rust/devcontainer.json"),
            None,
            Selection::NonInteractive,
        )?;
        assert_eq!(requested.path, ".devcontainer/rust/devcontainer.json");
        assert!(requested.persist);

        let interactive = select_config("/workspace", &configs, None, None, Selection::Index(1))?;
        assert_eq!(interactive.path, ".devcontainer/go/devcontainer.json");
        assert!(interactive.persist);
        assert!(
            select_config(
                "/workspace",
                &configs,
                None,
                None,
                Selection::NonInteractive
            )
            .is_err()
        );
        Ok(())
    }
}
