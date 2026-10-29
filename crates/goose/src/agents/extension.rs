use crate::config::permission::PermissionLevel;

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: Vec<String>,
    pub permission: Option<PermissionLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

impl ToolInfo {
    pub fn new(
        name: &str,
        description: &str,
        parameters: Vec<String>,
        permission: Option<PermissionLevel>,
    ) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
            permission,
            input_schema: None,
        }
    }

    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::config;
    use goose_agent::extension::{Envs, ExtensionConfig};
    use test_case::test_case;

    #[test]
    fn test_deserialize_missing_description() {
        let config: ExtensionConfig = serde_yaml::from_str(
            "enabled: true
type: builtin
name: developer
display_name: Developer
timeout: 300
bundled: true
available_tools: []",
        )
        .unwrap();
        if let ExtensionConfig::Builtin { description, .. } = config {
            assert_eq!(description, "")
        } else {
            panic!("unexpected result of deserialization: {}", config)
        }
    }

    #[test]
    fn test_deserialize_null_description() {
        let config: ExtensionConfig = serde_yaml::from_str(
            "enabled: true
type: builtin
name: developer
display_name: Developer
description: null
timeout: 300
bundled: true
available_tools: []
",
        )
        .unwrap();
        if let ExtensionConfig::Builtin { description, .. } = config {
            assert_eq!(description, "")
        } else {
            panic!("unexpected result of deserialization: {}", config)
        }
    }

    #[test]
    fn test_deserialize_normal_description() {
        let config: ExtensionConfig = serde_yaml::from_str(
            "enabled: true
type: builtin
name: developer
display_name: Developer
description: description goes here
timeout: 300
bundled: true
available_tools: []
    ",
        )
        .unwrap();
        if let ExtensionConfig::Builtin { description, .. } = config {
            assert_eq!(description, "description goes here")
        } else {
            panic!("unexpected result of deserialization: {}", config)
        }
    }

    #[test]
    fn test_deserialize_streamable_http_oauth_client_fields() {
        let config: ExtensionConfig = serde_yaml::from_str(
            "type: streamable_http
name: remote
uri: https://example.com/mcp
client_id: registered-client.example
client_secret_key: OAUTH_CLIENT_SECRET
scopes:
  - scope.read
  - scope.write
timeout: 300",
        )
        .unwrap();

        let ExtensionConfig::StreamableHttp {
            client_id,
            client_secret_key,
            scopes,
            ..
        } = config
        else {
            panic!("expected streamable_http config");
        };

        assert_eq!(client_id.as_deref(), Some("registered-client.example"));
        assert_eq!(client_secret_key.as_deref(), Some("OAUTH_CLIENT_SECRET"));
        assert_eq!(scopes, vec!["scope.read", "scope.write"]);
    }

    #[test]
    fn test_deserialize_streamable_http_without_oauth_client_fields() {
        let config: ExtensionConfig = serde_yaml::from_str(
            "type: streamable_http
name: remote
uri: https://example.com/mcp
timeout: 300",
        )
        .unwrap();

        let ExtensionConfig::StreamableHttp {
            client_id,
            client_secret_key,
            scopes,
            ..
        } = config
        else {
            panic!("expected streamable_http config");
        };

        assert_eq!(client_id, None);
        assert_eq!(client_secret_key, None);
        assert!(scopes.is_empty());
    }

    #[test]
    fn serialization_omits_unset_oauth_client_fields() {
        let config = ExtensionConfig::streamable_http(
            "remote",
            "https://example.com/mcp",
            "remote extension",
            300u64,
        );

        let yaml = serde_yaml::to_string(&config).unwrap();

        assert!(!yaml.contains("client_id"));
        assert!(!yaml.contains("client_secret_key"));
        assert!(!yaml.contains("scopes"));
    }

    #[test]
    fn envs_deserialization_filters_disallowed_keys() {
        let envs: Envs =
            serde_yaml::from_str("LD_PRELOAD: /tmp/injected.so\nSAFE_VAR: ok\n").unwrap();
        let map = envs.get_env();

        assert!(!map.contains_key("LD_PRELOAD"));
        assert_eq!(map.get("SAFE_VAR"), Some(&"ok".to_string()));
    }

    #[test]
    fn serialization_omits_empty_available_tools() {
        let config = ExtensionConfig::Builtin {
            name: "developer".into(),
            description: "dev".into(),
            display_name: Some("Developer".into()),
            timeout: Some(300),
            bundled: Some(true),
            available_tools: vec![],
        };

        let yaml = serde_yaml::to_string(&config).unwrap();

        assert!(!yaml.contains("available_tools"));
    }

    #[test]
    fn serialization_preserves_available_tools() {
        let config = ExtensionConfig::Builtin {
            name: "developer".into(),
            description: "dev".into(),
            display_name: Some("Developer".into()),
            timeout: Some(300),
            bundled: Some(true),
            available_tools: vec!["shell".to_string()],
        };

        let yaml = serde_yaml::to_string(&config).unwrap();

        assert!(yaml.contains("available_tools"));
        assert!(yaml.contains("- shell"));
    }

    #[test_case(
        ExtensionConfig::Builtin {
            name: "developer".into(),
            description: "dev".into(),
            display_name: None,
            timeout: None,
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::Builtin {
            name: "developer".into(),
            description: "dev".into(),
            display_name: None,
            timeout: None,
            bundled: None,
            available_tools: vec![],
        }
        ; "builtin_unchanged"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com".into(),
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("AUTH_TOKEN".to_string(), "secret".to_string());
                m
            }),
            env_keys: vec![],
            headers: [(
                "Authorization".to_string(),
                "Bearer $AUTH_TOKEN".to_string(),
            )]
            .into_iter()
            .collect(),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com".into(),
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("AUTH_TOKEN".to_string(), "secret".to_string());
                m
            }),
            env_keys: vec![],
            headers: [(
                "Authorization".to_string(),
                "Bearer secret".to_string(),
            )]
            .into_iter()
            .collect(),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        }
        ; "header_substitution"
    )]
    #[test_case(
        ExtensionConfig::Stdio {
            name: "test".into(),
            description: String::new(),
            cmd: "echo".into(),
            args: vec![],
            envs: Envs::default(),
            env_keys: vec![],
            timeout: None,
            cwd: None,
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::Stdio {
            name: "test".into(),
            description: String::new(),
            cmd: "echo".into(),
            args: vec![],
            envs: Envs::default(),
            env_keys: vec![],
            timeout: None,
            cwd: None,
            bundled: None,
            available_tools: vec![],
        }
        ; "env_keys_cleared"
    )]
    #[test_case(
        ExtensionConfig::Stdio {
            name: "test".into(),
            description: String::new(),
            cmd: "echo".into(),
            args: vec![],
            envs: Envs::default(),
            env_keys: vec!["MY_SECRET".into()],
            timeout: None,
            cwd: None,
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::Stdio {
            name: "test".into(),
            description: String::new(),
            cmd: "echo".into(),
            args: vec![],
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("MY_SECRET".to_string(), "secret_value".to_string());
                m
            }),
            env_keys: vec![],
            timeout: None,
            cwd: None,
            bundled: None,
            available_tools: vec![],
        }
        ; "env_key_resolved"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com".into(),
            envs: Envs::default(),
            env_keys: vec!["MY_SECRET".into()],
            headers: [(
                "Authorization".to_string(),
                "Bearer $MY_SECRET".to_string(),
            )]
            .into_iter()
            .collect(),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com".into(),
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("MY_SECRET".to_string(), "secret_value".to_string());
                m
            }),
            env_keys: vec![],
            headers: [("Authorization".to_string(), "Bearer secret_value".to_string())]
                .into_iter()
                .collect(),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        }
        ; "http_env_key_and_header_substitution"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com/mcp?api_key=$MY_SECRET".into(),
            envs: Envs::default(),
            env_keys: vec!["MY_SECRET".into()],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com/mcp?api_key=secret_value".into(),
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("MY_SECRET".to_string(), "secret_value".to_string());
                m
            }),
            env_keys: vec![],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        }
        ; "http_env_key_uri_substitution"
    )]
    #[test_case(
        ExtensionConfig::Stdio {
            name: "test".into(),
            description: String::new(),
            cmd: "echo".into(),
            args: vec![],
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("MY_SECRET".to_string(), "original".to_string());
                m
            }),
            env_keys: vec!["MY_SECRET".into()],
            timeout: None,
            cwd: None,
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::Stdio {
            name: "test".into(),
            description: String::new(),
            cmd: "echo".into(),
            args: vec![],
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("MY_SECRET".to_string(), "original".to_string());
                m
            }),
            env_keys: vec![],
            timeout: None,
            cwd: None,
            bundled: None,
            available_tools: vec![],
        }
        ; "env_key_skipped_when_already_in_envs"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com/mcp".into(),
            envs: Envs::default(),
            env_keys: vec!["MY_SECRET".into()],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: None,
            client_id: Some("${MY_SECRET}".into()),
            client_secret_key: Some("MY_SECRET".into()),
            scopes: vec!["scope.read".into()],
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com/mcp".into(),
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("MY_SECRET".to_string(), "secret_value".to_string());
                m
            }),
            env_keys: vec![],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: None,
            client_id: Some("secret_value".into()),
            client_secret_key: Some("MY_SECRET".into()),
            scopes: vec!["scope.read".into()],
            bundled: None,
            available_tools: vec![],
        }
        ; "http_client_id_substitution_and_oauth_fields_preserved"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com/mcp".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: None,
            client_id: Some("registered-client".into()),
            client_secret_key: Some("MY_SECRET".into()),
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "https://example.com/mcp".into(),
            envs: Envs::new({
                let mut m = std::collections::HashMap::new();
                m.insert("MY_SECRET".to_string(), "secret_value".to_string());
                m
            }),
            env_keys: vec![],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: None,
            client_id: Some("registered-client".into()),
            client_secret_key: Some("MY_SECRET".into()),
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        }
        ; "http_client_secret_key_resolved_without_env_keys_entry"
    )]
    #[tokio::test]
    async fn test_resolve(config: ExtensionConfig, expected: ExtensionConfig) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config::Config::new_with_file_secrets(
            dir.path().join("config.yaml"),
            dir.path().join("secrets.yaml"),
        )
        .unwrap();
        cfg.set("MY_SECRET", &"secret_value", true).unwrap();
        assert_eq!(config.resolve(&cfg).await.unwrap(), expected);
    }

    #[tokio::test]
    async fn test_resolve_takes_env_overrides_as_raw_strings() {
        let _guard = env_lock::lock_env([("PORT", Some("3000")), ("HEADLESS", Some("true"))]);
        let dir = tempfile::tempdir().unwrap();
        let cfg = config::Config::new_with_file_secrets(
            dir.path().join("config.yaml"),
            dir.path().join("secrets.yaml"),
        )
        .unwrap();
        let config = ExtensionConfig::Stdio {
            name: "test".to_string(),
            description: String::new(),
            cmd: "cmd".to_string(),
            args: vec![],
            envs: Envs::default(),
            env_keys: vec!["PORT".to_string(), "HEADLESS".to_string()],
            timeout: None,
            cwd: None,
            bundled: None,
            available_tools: vec![],
        };

        let resolved = config.resolve(&cfg).await.unwrap();

        let ExtensionConfig::Stdio { envs, .. } = resolved else {
            panic!("expected stdio config");
        };
        let envs = envs.get_env();
        assert_eq!(envs["PORT"], "3000");
        assert_eq!(envs["HEADLESS"], "true");
    }

    #[test]
    fn test_display_streamable_http_with_socket() {
        let config = ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "http://localhost:8080/mcp".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: Some("@egress.sock".to_string()),
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        };
        assert_eq!(
            config.to_string(),
            "StreamableHttp(test: http://localhost:8080/mcp via @egress.sock)"
        );
    }

    #[test]
    fn test_display_streamable_http_without_socket() {
        let config = ExtensionConfig::StreamableHttp {
            name: "test".into(),
            description: String::new(),
            uri: "http://localhost:8080/mcp".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: std::collections::HashMap::new(),
            timeout: None,
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: None,
            available_tools: vec![],
        };
        assert_eq!(
            config.to_string(),
            "StreamableHttp(test: http://localhost:8080/mcp)"
        );
    }
}
