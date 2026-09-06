use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum RawForwardPort {
    Number(u64),
    Host(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForwardPort {
    Number(u16),
    Host { host: String, port: u32 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum AppPort {
    Number(serde_json::Number),
    String(String),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum PortProtocol {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "https")]
    Https,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum AutoForwardAction {
    #[serde(rename = "notify")]
    Notify,
    #[serde(rename = "openBrowser")]
    OpenBrowser,
    #[serde(rename = "openBrowserOnce")]
    OpenBrowserOnce,
    #[serde(rename = "openPreview")]
    OpenPreview,
    #[serde(rename = "silent")]
    Silent,
    #[serde(rename = "ignore")]
    Ignore,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PortAttributes {
    pub label: Option<String>,
    pub protocol: Option<PortProtocol>,
    pub on_auto_forward: Option<AutoForwardAction>,
    pub require_local_port: Option<bool>,
    pub elevate_if_needed: Option<bool>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}
