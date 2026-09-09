---
title: Azure AI Foundry
description: Use OpenAI, Anthropic, and partner model deployments from an Azure AI Foundry project
---

# Azure AI Foundry

The `azure_foundry` provider connects goose to Azure AI Foundry deployments. It supports two endpoint types:

| Endpoint | Inference surface |
|---|---|
| Foundry project: `https://<resource>.services.ai.azure.com/api/projects/<project>` | Deployment discovery plus publisher-aware routing |
| Foundry resource: `https://<resource>.services.ai.azure.com` | OpenAI models through Responses, Claude through Anthropic Messages, and partner models through Chat Completions |
| MaaS/serverless: `https://<deployment>.<region>.models.ai.azure.com` | Chat Completions for the model bound to the endpoint |

For project endpoints, goose discovers deployments with `GET /deployments`. Deployment names can be customized; goose uses the returned `modelPublisher` to select the protocol and `modelName` to resolve model metadata such as the context window.

Resource endpoints do not expose project deployment discovery. goose routes recognizable model or deployment names by family: `gpt-5*` and supported o-series models use Responses, `claude-*` uses Anthropic Messages, and other names use Chat Completions. Use a project endpoint when aliases do not identify their underlying model family.

## Configuration

| Variable | Required | Description |
|---|---:|---|
| `AZURE_FOUNDRY_ENDPOINT` | Yes | Full Foundry project or MaaS endpoint |
| `AZURE_FOUNDRY_API_KEY` | No | API key; omit it to use Azure CLI credentials |
| `AZURE_FOUNDRY_MODEL` | MaaS only | Model bound to the configured MaaS endpoint |
| `AZURE_FOUNDRY_AD_TOKEN` | No | Pre-acquired Microsoft Entra access token; takes precedence over the API key |
| `AZURE_FOUNDRY_API_VERSION` | No | Deployment discovery API version; project endpoints default to `v1` |

Run `goose configure`, select **Configure Providers**, and choose **Azure AI Foundry**. You can also set the variables before starting goose:

```sh
export AZURE_FOUNDRY_ENDPOINT="https://my-resource.services.ai.azure.com/api/projects/my-project"
export AZURE_FOUNDRY_API_KEY="<key>"
goose session
```

For a MaaS endpoint:

```sh
export AZURE_FOUNDRY_ENDPOINT="https://my-deployment.eastus.models.ai.azure.com"
export AZURE_FOUNDRY_API_KEY="<key>"
export AZURE_FOUNDRY_MODEL="<model-bound-to-this-endpoint>"
goose session
```

MaaS endpoints expose a single deployed model. `AZURE_FOUNDRY_MODEL` is required for these endpoints.

## Authentication

Authentication is selected in this order:

1. `AZURE_FOUNDRY_AD_TOKEN`
2. `AZURE_FOUNDRY_API_KEY`
3. Azure CLI credentials

When neither token nor key is configured, sign in with Azure CLI before starting goose:

```sh
az login
```

Project and resource endpoints request a token for `https://ai.azure.com`. MaaS endpoints request a token for `https://ml.azure.com`.

## Protocol routing

For a project endpoint, goose routes each deployment using metadata returned by Azure:

- publisher `OpenAI` with a Responses-compatible model (`gpt-5*` and the supported o-series) → `/openai/v1/responses`
- publisher `Anthropic` → `/anthropic/v1/messages`
- older OpenAI models and all other publishers → `/openai/v1/chat/completions`

If deployment discovery is temporarily unavailable, recognizable Responses-compatible OpenAI and `claude-*` names use their native surfaces. Other names use Chat Completions.

Resource endpoints use the same inference surfaces without deployment discovery: recognizable model-family names select the native protocol. This allows a custom agent's `model:` declaration to select deployments such as `gpt-5.6-sol` without rewriting the name through canonical model resolution.

MaaS endpoints always use `/v1/chat/completions` and the model configured by `AZURE_FOUNDRY_MODEL`.

## Model metadata and pricing

The deployments API provides the deployment name and underlying `modelName`, `modelVersion`, and `modelPublisher`. goose uses the underlying model name to look up a context window in its bundled model catalog. An explicit `GOOSE_CONTEXT_LIMIT` or session override still takes precedence.

Azure pricing depends on region, SKU, offer, deployment type, and contract. The deployments API does not provide a reliable per-token price, so this provider does not attach a price to discovered deployments.

## Troubleshooting

### 401 or 403

- Ensure the key belongs to the configured endpoint.
- For Entra authentication, run `az login` again and verify that your identity has access to the Foundry project.
- Do not use a project endpoint key with a MaaS endpoint, or the reverse.

### No deployments are listed

- Confirm that the endpoint includes `/api/projects/<project>`.
- Confirm that the project contains model deployments.
- If your project uses a non-default deployment API version, set `AZURE_FOUNDRY_API_VERSION`.

### Wrong protocol for a custom deployment name

Refresh the provider model list so goose can retrieve `modelPublisher`. Without deployment metadata, routing can only use recognizable model-name prefixes.
