use crate::agents::extension::{Envs, ExtensionConfig};
use rmcp::model::Tool;
use serde::de::Deserializer;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(tag = "type")]
enum RecipeExtensionConfigInternal {
    #[serde(rename = "stdio")]
    Stdio {
        name: String,
        #[serde(default)]
        description: Option<String>,
        cmd: String,
        args: Vec<String>,
        #[serde(default)]
        envs: Envs,
        #[serde(default)]
        env_keys: Vec<String>,
        timeout: Option<u64>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        available_tools: Vec<String>,
    },
    #[serde(rename = "builtin")]
    Builtin {
        name: String,
        #[serde(default)]
        description: Option<String>,
        display_name: Option<String>,
        timeout: Option<u64>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        available_tools: Vec<String>,
    },
    #[serde(rename = "platform")]
    Platform {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        available_tools: Vec<String>,
    },
    #[serde(rename = "streamable_http")]
    StreamableHttp {
        name: String,
        #[serde(default)]
        description: Option<String>,
        uri: String,
        #[serde(default)]
        envs: Envs,
        #[serde(default)]
        env_keys: Vec<String>,
        #[serde(default)]
        headers: HashMap<String, String>,
        timeout: Option<u64>,
        #[serde(default)]
        socket: Option<String>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        available_tools: Vec<String>,
    },
    #[serde(rename = "frontend")]
    Frontend {
        name: String,
        #[serde(default)]
        description: Option<String>,
        tools: Vec<Tool>,
        instructions: Option<String>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        available_tools: Vec<String>,
    },
    #[serde(rename = "inline_python")]
    InlinePython {
        name: String,
        #[serde(default)]
        description: Option<String>,
        code: String,
        timeout: Option<u64>,
        #[serde(default)]
        dependencies: Option<Vec<String>>,
        #[serde(default)]
        available_tools: Vec<String>,
    },
}

macro_rules! map_recipe_extensions {
    ($value:expr; $( $variant:ident { $( $field:ident ),* $(,)? } ),+ $(,)?) => {{
        match $value {
            $(
                RecipeExtensionConfigInternal::$variant {
                    name,
                    description,
                    $( $field ),*
                } => ExtensionConfig::$variant {
                    name,
                    description: description.unwrap_or_default(),
                    $( $field ),*
                },
            )+
        }
    }};
}

impl From<RecipeExtensionConfigInternal> for ExtensionConfig {
    fn from(internal_variant: RecipeExtensionConfigInternal) -> Self {
        map_recipe_extensions!(
        internal_variant;
            Stdio {
                cmd,
                args,
                envs,
                env_keys,
                timeout,
                bundled,
                available_tools
            },
            Builtin {
                display_name,
                timeout,
                bundled,
                available_tools
            },
            Platform {
                display_name,
                bundled,
                available_tools
            },
            StreamableHttp {
                uri,
                envs,
                env_keys,
                headers,
                timeout,
                socket,
                bundled,
                available_tools
            },
            Frontend {
                tools,
                instructions,
                bundled,
                available_tools
            },
            InlinePython {
                code,
                timeout,
                dependencies,
                available_tools
            }
        )
    }
}

pub fn deserialize_recipe_extensions<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<ExtensionConfig>>, D::Error>
where
    D: Deserializer<'de>,
{
    let remotes = Option::<Vec<RecipeExtensionConfigInternal>>::deserialize(deserializer)?;
    Ok(remotes.map(|items| items.into_iter().map(ExtensionConfig::from).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(deserialize_with = "deserialize_recipe_extensions")]
        extensions: Option<Vec<ExtensionConfig>>,
    }

    #[test]
    fn builtin_extension_defaults_description() {
        let wrapper: Wrapper = serde_json::from_value(json!({
            "extensions": [{
                "type": "builtin",
                "name": "test-builtin",
                "display_name": "Test Builtin",
                "timeout": 120,
                "bundled": true,
                "available_tools": ["tool_a", "tool_b"],
            }]
        }))
        .expect("failed to deserialize extensions");

        let extensions = wrapper.extensions.expect("expected extensions");
        assert_eq!(extensions.len(), 1);

        match &extensions[0] {
            ExtensionConfig::Builtin {
                name,
                description,
                display_name,
                timeout,
                bundled,
                available_tools,
            } => {
                assert_eq!(name, "test-builtin");
                assert_eq!(description, "");
                assert_eq!(display_name.as_deref(), Some("Test Builtin"));
                assert_eq!(*timeout, Some(120));
                assert_eq!(*bundled, Some(true));
                assert_eq!(
                    available_tools,
                    &vec!["tool_a".to_string(), "tool_b".to_string()]
                );
            }
            other => panic!("unexpected extension variant: {:?}", other),
        }
    }

    #[test]
    fn builtin_extension_null_description_defaults_to_empty() {
        let wrapper: Wrapper = serde_json::from_value(json!({
            "extensions": [{
                "type": "builtin",
                "name": "null-description-builtin",
                "description": null,
            }]
        }))
        .expect("failed to deserialize extensions with null description");

        let extensions = wrapper.extensions.expect("expected extensions");
        assert_eq!(extensions.len(), 1);

        match &extensions[0] {
            ExtensionConfig::Builtin {
                name,
                description,
                display_name,
                timeout,
                bundled,
                available_tools,
            } => {
                assert_eq!(name, "null-description-builtin");
                assert_eq!(description, "");
                assert!(display_name.is_none());
                assert!(timeout.is_none());
                assert!(bundled.is_none());
                assert!(available_tools.is_empty());
            }
            other => panic!("unexpected extension variant: {:?}", other),
        }
    }
}
