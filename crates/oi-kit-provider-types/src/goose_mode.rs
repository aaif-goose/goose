use serde::{Deserialize, Serialize};
use strum::{Display, EnumMessage, EnumString, IntoStaticStr, VariantNames};

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    Eq,
    Hash,
    PartialEq,
    Serialize,
    Deserialize,
    Display,
    EnumMessage,
    EnumString,
    IntoStaticStr,
    VariantNames,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum GooseMode {
    #[default]
    #[strum(message = "Automatically approve tool calls")]
    Auto,
    #[serde(alias = "chat")]
    #[strum(
        to_string = "approve",
        serialize = "chat",
        message = "Ask before every tool call"
    )]
    Approve,
    #[strum(message = "Ask only for sensitive tool calls")]
    SmartApprove,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_chat_mode_loads_as_approve() {
        assert_eq!("chat".parse::<GooseMode>().unwrap(), GooseMode::Approve);
        assert_eq!(
            serde_json::from_str::<GooseMode>("\"chat\"").unwrap(),
            GooseMode::Approve
        );
        assert_eq!(GooseMode::Approve.to_string(), "approve");
        assert_eq!(GooseMode::VARIANTS, ["auto", "approve", "smart_approve"]);
    }
}
