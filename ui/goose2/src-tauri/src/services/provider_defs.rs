pub(crate) struct ConfigKey {
    pub name: &'static str,
    pub is_secret: bool,
    pub required: bool,
}

pub(crate) struct ProviderConfigDef {
    pub id: &'static str,
    pub keys: &'static [ConfigKey],
    pub oauth_cache_path: Option<&'static str>,
}

const fn key(name: &'static str, is_secret: bool, required: bool) -> ConfigKey {
    ConfigKey {
        name,
        is_secret,
        required,
    }
}

pub(crate) static PROVIDER_CONFIG_DEFS: &[ProviderConfigDef] = &[
    ProviderConfigDef {
        id: "anthropic",
        keys: &[key("ANTHROPIC_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "openai",
        keys: &[key("OPENAI_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "google",
        keys: &[key("GOOGLE_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "openrouter",
        keys: &[key("OPENROUTER_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "xai",
        keys: &[key("XAI_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "nanogpt",
        keys: &[key("NANOGPT_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "venice",
        keys: &[key("VENICE_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "tetrate",
        keys: &[key("TETRATE_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "databricks",
        keys: &[
            key("DATABRICKS_HOST", false, true),
            key("DATABRICKS_TOKEN", true, false),
        ],
        oauth_cache_path: Some("databricks/oauth"),
    },
    ProviderConfigDef {
        id: "snowflake",
        keys: &[
            key("SNOWFLAKE_HOST", false, true),
            key("SNOWFLAKE_TOKEN", true, true),
        ],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "litellm",
        keys: &[
            key("LITELLM_HOST", false, true),
            key("LITELLM_API_KEY", true, false),
        ],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "azure",
        keys: &[
            key("AZURE_OPENAI_ENDPOINT", false, true),
            key("AZURE_OPENAI_DEPLOYMENT_NAME", false, true),
            key("AZURE_OPENAI_API_KEY", true, false),
        ],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "bedrock",
        keys: &[key("AWS_REGION", false, false)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "gcp_vertex_ai",
        keys: &[
            key("GCP_PROJECT_ID", false, true),
            key("GCP_LOCATION", false, true),
        ],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "chatgpt_codex",
        keys: &[key("CHATGPT_CODEX_TOKEN", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "github_copilot",
        keys: &[],
        oauth_cache_path: Some("githubcopilot/info.json"),
    },
    ProviderConfigDef {
        id: "ollama",
        keys: &[],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "local_inference",
        keys: &[],
        oauth_cache_path: None,
    },
    // Dictation providers (voice input)
    ProviderConfigDef {
        id: "dictation_groq",
        keys: &[key("GROQ_API_KEY", true, true)],
        oauth_cache_path: None,
    },
    ProviderConfigDef {
        id: "dictation_elevenlabs",
        keys: &[key("ELEVENLABS_API_KEY", true, true)],
        oauth_cache_path: None,
    },
];

pub(crate) fn find_config_key(key_name: &str) -> Option<&'static ConfigKey> {
    PROVIDER_CONFIG_DEFS
        .iter()
        .flat_map(|def| def.keys.iter())
        .find(|key| key.name == key_name)
}

pub(crate) fn find_provider_def(provider_id: &str) -> Option<&'static ProviderConfigDef> {
    PROVIDER_CONFIG_DEFS.iter().find(|d| d.id == provider_id)
}
