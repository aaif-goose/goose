import type { ProviderCatalogEntry } from "@/shared/types/providers";
import {
  AGENT_PROVIDER_ALIAS_MAP,
  AGENT_PROVIDER_FUZZY_MATCHERS,
  normalizeProviderKey,
} from "./providerCatalogAliases";

export const PROVIDER_CATALOG: ProviderCatalogEntry[] = [
  // ── Agent providers ──────────────────────────────────────────────
  {
    id: "goose",
    displayName: "Goose",
    category: "agent",
    description: "Block's open-source coding agent",
    setupMethod: "none",
    tier: "promoted",
  },
  {
    id: "claude-acp",
    displayName: "Claude Code",
    category: "agent",
    description: "Anthropic's agentic coding tool",
    setupMethod: "cli_auth",
    binaryName: "claude-agent-acp",
    installCommand:
      "npm install -g @anthropic-ai/claude-code @zed-industries/claude-agent-acp",
    authCommand: "claude auth login",
    authStatusCommand: "claude auth status",
    docsUrl: "https://docs.anthropic.com/en/docs/claude-code",
    tier: "promoted",
  },
  {
    id: "codex-acp",
    displayName: "Codex",
    category: "agent",
    description: "OpenAI's coding agent",
    setupMethod: "cli_auth",
    binaryName: "codex-acp",
    installCommand: "npm install -g @openai/codex @zed-industries/codex-acp",
    authCommand: "codex login",
    authStatusCommand: "codex login status",
    docsUrl: "https://github.com/openai/codex",
    tier: "promoted",
  },
  {
    id: "copilot-acp",
    displayName: "GitHub Copilot",
    category: "agent",
    description: "GitHub's AI pair programmer",
    setupMethod: "cli_auth",
    binaryName: "copilot",
    installCommand: "npm install -g @github/copilot",
    authCommand: "copilot login",
    docsUrl: "https://docs.github.com/en/copilot/github-copilot-in-the-cli",
    tier: "promoted",
  },
  {
    id: "amp-acp",
    displayName: "Amp",
    category: "agent",
    description: "Sourcegraph's coding agent",
    setupMethod: "cli_auth",
    binaryName: "amp-acp",
    installCommand: "npm install -g @sourcegraph/amp@latest amp-acp",
    authCommand: "amp login",
    authStatusCommand: "amp usage",
    docsUrl: "https://ampcode.com",
    tier: "standard",
  },
  {
    id: "cursor-agent",
    displayName: "Cursor Agent",
    category: "agent",
    description: "Cursor's AI agent",
    setupMethod: "cli_auth",
    binaryName: "cursor-agent",
    installCommand: "curl -fsSL https://cursor.com/install | bash",
    authCommand: "cursor-agent login",
    authStatusCommand: "cursor-agent status",
    docsUrl: "https://docs.cursor.com/en/cli/overview",
    tier: "standard",
  },
  {
    id: "pi-acp",
    displayName: "Pi",
    category: "agent",
    description: "Open-source AI coding agent",
    setupMethod: "cli_auth",
    binaryName: "pi-acp",
    docsUrl: "https://github.com/badlogic/pi-mono",
    tier: "standard",
    showOnlyWhenInstalled: true,
  },

  // ── Model providers (power Goose) ────────────────────────────────
  {
    id: "anthropic",
    displayName: "Anthropic",
    category: "model",
    description: "Claude models",
    setupMethod: "single_api_key",
    envVar: "ANTHROPIC_API_KEY",
    fields: [
      {
        key: "ANTHROPIC_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    docsUrl: "https://console.anthropic.com/settings/keys",
    tier: "promoted",
  },
  {
    id: "google",
    displayName: "Google Gemini",
    category: "model",
    description: "Gemini models",
    setupMethod: "single_api_key",
    envVar: "GOOGLE_API_KEY",
    fields: [
      {
        key: "GOOGLE_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    docsUrl: "https://aistudio.google.com/apikey",
    tier: "promoted",
  },
  {
    id: "chatgpt_codex",
    displayName: "ChatGPT Codex",
    category: "model",
    description: "OpenAI via ChatGPT subscription",
    setupMethod: "oauth_device_code",
    nativeConnectQuery: "ChatGPT Codex",
    docsUrl: "https://chatgpt.com",
    tier: "standard",
  },
  {
    id: "openai",
    displayName: "OpenAI",
    category: "model",
    description: "GPT and o-series models",
    setupMethod: "config_fields",
    envVar: "OPENAI_API_KEY",
    fields: [
      {
        key: "OPENAI_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    docsUrl: "https://platform.openai.com/api-keys",
    tier: "promoted",
  },
  {
    id: "ollama",
    displayName: "Ollama",
    category: "model",
    description: "Run models locally",
    setupMethod: "local",
    docsUrl: "https://ollama.com",
    tier: "promoted",
  },
  {
    id: "openrouter",
    displayName: "OpenRouter",
    category: "model",
    description: "Unified API for many models",
    setupMethod: "single_api_key",
    envVar: "OPENROUTER_API_KEY",
    fields: [
      {
        key: "OPENROUTER_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    docsUrl: "https://openrouter.ai/keys",
    tier: "promoted",
  },
  {
    id: "databricks",
    displayName: "Databricks",
    category: "model",
    description: "Databricks Foundation Models",
    setupMethod: "host_with_oauth_fallback",
    fields: [
      {
        key: "DATABRICKS_HOST",
        label: "Host URL",
        secret: false,
        required: true,
        placeholder: "https://dbc-...cloud.databricks.com",
      },
      {
        key: "DATABRICKS_TOKEN",
        label: "Access Token",
        secret: true,
        required: false,
        placeholder: "Paste your access token",
      },
    ],
    tier: "standard",
  },
  {
    id: "github_copilot",
    displayName: "GitHub Copilot Models",
    category: "model",
    description: "Models via GitHub Copilot subscription",
    setupMethod: "oauth_device_code",
    nativeConnectQuery: "GitHub Copilot",
    tier: "standard",
  },
  {
    id: "xai",
    displayName: "xAI",
    category: "model",
    description: "Grok models",
    setupMethod: "single_api_key",
    envVar: "XAI_API_KEY",
    fields: [
      {
        key: "XAI_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    tier: "standard",
  },
  {
    id: "azure",
    displayName: "Azure OpenAI",
    category: "model",
    description: "OpenAI models on Azure",
    setupMethod: "config_fields",
    fields: [
      {
        key: "AZURE_OPENAI_ENDPOINT",
        label: "Endpoint",
        secret: false,
        required: true,
        placeholder: "https://your-resource.openai.azure.com",
      },
      {
        key: "AZURE_OPENAI_DEPLOYMENT_NAME",
        label: "Deployment",
        secret: false,
        required: true,
        placeholder: "gpt-4o",
      },
      {
        key: "AZURE_OPENAI_API_KEY",
        label: "API Key",
        secret: true,
        required: false,
        placeholder: "Paste your API key",
      },
    ],
    tier: "advanced",
  },
  {
    id: "bedrock",
    displayName: "AWS Bedrock",
    category: "model",
    description: "Models on AWS",
    setupMethod: "cloud_credentials",
    fields: [
      {
        key: "AWS_REGION",
        label: "AWS Region",
        secret: false,
        required: false,
        placeholder: "us-west-2",
      },
    ],
    tier: "advanced",
  },
  {
    id: "gcp_vertex_ai",
    displayName: "GCP Vertex AI",
    category: "model",
    description: "Models on Google Cloud",
    setupMethod: "cloud_credentials",
    fields: [
      {
        key: "GCP_PROJECT_ID",
        label: "Project ID",
        secret: false,
        required: true,
        placeholder: "my-gcp-project",
      },
      {
        key: "GCP_LOCATION",
        label: "Location",
        secret: false,
        required: true,
        placeholder: "us-central1",
      },
    ],
    tier: "advanced",
  },
  {
    id: "litellm",
    displayName: "LiteLLM",
    category: "model",
    description: "LiteLLM proxy gateway",
    setupMethod: "config_fields",
    envVar: "LITELLM_API_KEY",
    fields: [
      {
        key: "LITELLM_HOST",
        label: "Host URL",
        secret: false,
        required: true,
        placeholder: "https://your-proxy.example.com",
      },
      {
        key: "LITELLM_API_KEY",
        label: "API Key",
        secret: true,
        required: false,
        placeholder: "Paste your API key",
      },
    ],
    tier: "advanced",
  },
  {
    id: "nanogpt",
    displayName: "NanoGPT",
    category: "model",
    description: "NanoGPT inference",
    setupMethod: "single_api_key",
    envVar: "NANOGPT_API_KEY",
    fields: [
      {
        key: "NANOGPT_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    tier: "advanced",
  },
  {
    id: "tetrate",
    displayName: "Tetrate",
    category: "model",
    description: "Tetrate AI gateway",
    setupMethod: "single_api_key",
    fields: [
      {
        key: "TETRATE_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    tier: "advanced",
  },
  {
    id: "venice",
    displayName: "Venice",
    category: "model",
    description: "Venice AI",
    setupMethod: "single_api_key",
    envVar: "VENICE_API_KEY",
    fields: [
      {
        key: "VENICE_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    tier: "advanced",
  },
  {
    id: "snowflake",
    displayName: "Snowflake",
    category: "model",
    description: "Snowflake Cortex",
    setupMethod: "config_fields",
    fields: [
      {
        key: "SNOWFLAKE_HOST",
        label: "Host URL",
        secret: false,
        required: true,
        placeholder: "https://your-account.snowflakecomputing.com",
      },
      {
        key: "SNOWFLAKE_TOKEN",
        label: "Access Token",
        secret: true,
        required: true,
        placeholder: "Paste your access token",
      },
    ],
    tier: "advanced",
  },
  {
    id: "local_inference",
    displayName: "Local Inference",
    category: "model",
    description: "Custom local model server",
    setupMethod: "local",
    tier: "advanced",
  },
];

export function getCatalogEntry(
  providerId: string,
): ProviderCatalogEntry | undefined {
  return PROVIDER_CATALOG.find((p) => p.id === providerId);
}

export function getAgentProviders(): ProviderCatalogEntry[] {
  return PROVIDER_CATALOG.filter((p) => p.category === "agent");
}

export function getModelProviders(): ProviderCatalogEntry[] {
  return PROVIDER_CATALOG.filter((p) => p.category === "model");
}

export function resolveAgentProviderCatalogIdStrict(
  providerId: string,
): string | null {
  const directMatch = getAgentProviders().find(
    (provider) => provider.id === providerId,
  );
  if (directMatch) {
    return directMatch.id;
  }

  const normalized = normalizeProviderKey(providerId);
  const aliasMatch = AGENT_PROVIDER_ALIAS_MAP[normalized];
  if (aliasMatch) {
    return aliasMatch;
  }

  return null;
}

export function resolveAgentProviderCatalogId(
  providerId: string,
  label?: string,
): string | null {
  const directMatch = getAgentProviders().find(
    (provider) => provider.id === providerId,
  );
  if (directMatch) {
    return directMatch.id;
  }

  const normalizedCandidates = [providerId, label ?? ""]
    .map((value) => normalizeProviderKey(value))
    .filter(Boolean);

  for (const candidate of normalizedCandidates) {
    const aliasMatch = AGENT_PROVIDER_ALIAS_MAP[candidate];
    if (aliasMatch) {
      return aliasMatch;
    }
  }

  for (const candidate of normalizedCandidates) {
    for (const [needle, catalogId] of AGENT_PROVIDER_FUZZY_MATCHERS) {
      if (candidate.includes(needle)) {
        return catalogId;
      }
    }
  }

  return null;
}
