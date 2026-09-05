pub mod doctor;
pub mod down;
pub mod list;
pub mod provider;
pub mod up;

/// An operation selected through the command-line interface.
#[derive(Debug, Eq, PartialEq)]
pub enum Action {
    /// Show configured workspaces and their observed state.
    List,
    /// Start one workspace target.
    Up {
        workspace: String,
        provider: Option<String>,
    },
    /// Stop one workspace target.
    Down {
        workspace: String,
        provider: Option<String>,
    },
    /// Show configured providers.
    ProviderList,
    /// Add a local or SSH provider.
    ProviderAdd {
        name: String,
        provider_type: String,
        host: Option<String>,
    },
    /// Delete an unused provider.
    ProviderDelete { name: String },
    /// Check whether the local Podman CLI can be executed.
    Doctor,
}
