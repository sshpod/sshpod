use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result, bail, ensure};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Selection {
    NonInteractive,
    Index(usize),
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
    use super::{Selection, select_provider};

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
}
