use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MountType {
    Bind,
    Volume,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawMountObject {
    #[serde(rename = "type")]
    pub kind: Option<MountType>,
    pub source: Option<String>,
    pub target: Option<String>,
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RawMount {
    String(String),
    Object(RawMountObject),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mount {
    String(String),
    Object {
        kind: MountType,
        source: Option<String>,
        target: String,
    },
}
