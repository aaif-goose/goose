<div align="center">

# oi-kit and goose

_Build agents that run anywhere with oi-kit, the Open Intelligence Kit._

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0"
    ><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg"></a>
  <a href="https://discord.gg/n8R5VaWDAn"
    ><img src="https://img.shields.io/discord/1287729918100246654?logo=discord&logoColor=white&label=Join+Us&color=blueviolet" alt="Discord"></a>
  <a href="https://github.com/aaif-goose/goose/actions/workflows/ci.yml"
     ><img src="https://img.shields.io/github/actions/workflow/status/aaif-goose/goose/ci.yml?branch=main" alt="CI"></a>
  <a href="https://insights.linuxfoundation.org/project/goose"><img src="https://insights.linuxfoundation.org/api/badge/health-score?project=goose"></a>
  <a href="https://repology.org/project/goose-cli/versions"><img src="https://repology.org/badge/tiny-repos/goose-cli.svg" alt="Packaging status"></a>
</p>

<a href="https://trendshift.io/repositories/25298?utm_source=repository-badge&amp;utm_medium=badge&amp;utm_campaign=badge-repository-25298" target="_blank" rel="noopener noreferrer"><img src="https://trendshift.io/api/badge/repositories/25298" alt="aaif-goose%2Fgoose | Trendshift" width="250" height="55"/></a>

</div>


[oi-kit (Open Intelligence Kit)](https://goose-docs.ai/docs/oi-kit/) provides the reusable Rust components for building agents. [goose](https://goose-docs.ai/docs/quickstart) is a complete agent built with oi-kit: use its desktop app or CLI, or make it the agent backend for your own application over ACP.

Both live in this repository. goose is part of the [Agentic AI Foundation (AAIF)](https://aaif.io/) at the Linux Foundation.

## Build with oi-kit

Compose an agent from a state machine, operations, tools, model providers, and context management. Your application owns the interface, policies, and storage.

- **Control how the agent works.** Compose operations for tool execution, approvals, compaction, and your application's own behavior.
- **Resume from saved state.** Supply your own storage and let the state machine determine what to do next from the persisted conversation.
- **Run in different environments.** Use the core runtime in native applications, on servers, or in WebAssembly hosts.
- **Run models in-process.** Load supported local models directly, without Ollama or a separate inference server, with control over model loading and inference.

Rust exposes the full agent runtime. Python and Kotlin bindings currently cover model providers, streaming, and conversation compaction. oi-kit is in alpha; use the documentation version that matches your dependencies.

- [Getting Started](https://goose-docs.ai/docs/oi-kit/hello-world) — build a small Rust agent with tools, streaming, and a saved conversation.
- [Demos](https://goose-docs.ai/docs/oi-kit/demos) — explore working applications and their source code.
- [Concepts](https://goose-docs.ai/docs/oi-kit/concepts) — understand the state machine, operations, providers, and authentication.
- [Reference](https://goose-docs.ai/docs/oi-kit/reference) — choose crates and find generated Rust, Python, and Kotlin API documentation.

## Use goose

goose is a general-purpose agent for coding, research, writing, automation, and data analysis. Use the native desktop app on macOS, Linux, or Windows, or work from the terminal with the CLI. Connect tools and data through the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/).

Load local models directly inside goose, without a separate model server, or connect to [over 85 providers](https://goose-docs.ai/docs/getting-started/providers), including Anthropic, OpenAI, Gemini, OpenRouter, and Amazon Bedrock.

**[Download the desktop app](https://goose-docs.ai/docs/getting-started/installation)** for macOS, Linux, and Windows.

Or install the CLI:

```bash
curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | bash
```

Then follow the [goose Quickstart](https://goose-docs.ai/docs/quickstart) to configure a model and start a session.

## Build an app over ACP

Use the pre-built goose binary as the agent behind your own application through the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/). Start it as a subprocess:

```bash
goose acp
```

Your app sends prompts and receives streamed responses, tool activity, and permission requests. goose supplies the agent, model connections, tools, and saved conversations; your app owns the interface and workflow. This is also how goose Desktop talks to its backend.

Follow [Build an app over ACP](https://goose-docs.ai/docs/oi-kit/acp/) for a working client example, then consult the [ACP extensions reference](https://goose-docs.ai/docs/oi-kit/acp/reference) for goose-specific capabilities.

## Quick links

- [Quickstart](https://goose-docs.ai/docs/quickstart)
- [Installation](https://goose-docs.ai/docs/getting-started/installation)
- [Tutorials](https://goose-docs.ai/docs/category/tutorials)
- [Documentation](https://goose-docs.ai/docs/category/getting-started)
- [Contributing](CONTRIBUTING.md)
- [Governance](https://github.com/aaif-goose/goose/blob/main/GOVERNANCE.md)
- [Custom Distributions](https://github.com/aaif-goose/goose/blob/main/CUSTOM_DISTROS.md) — build your own goose distro with preconfigured providers, extensions, and branding

## Need help?

- [Diagnostics & Reporting](https://goose-docs.ai/docs/troubleshooting/diagnostics-and-reporting)
- [Known Issues](https://goose-docs.ai/docs/troubleshooting/known-issues)

## Community

- [Discord](https://discord.gg/n8R5VaWDAn)
- [YouTube](https://www.youtube.com/@goose-oss)
- [LinkedIn](https://www.linkedin.com/company/goose-oss)
- [Twitter/X](https://x.com/goose_oss)
