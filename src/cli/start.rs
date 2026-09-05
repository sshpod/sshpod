use anyhow::Result;

use crate::cli::{actions::Action, commands, dispatch};

/// Parse the process arguments and return the selected action.
///
/// # Errors
///
/// Returns an error if parsed arguments cannot be dispatched to an action.
pub fn start() -> Result<Action> {
    let matches = commands::new().get_matches();
    dispatch::handler(&matches)
}
