<div align="center">

# goose

_Your open source AI agent._

Use goose on desktop or in the terminal, or build agents into your own applications with the [goose Development Kit (GDK)](https://goose-docs.ai/docs/gdk/).

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="Apache 2.0 license"></a>
  <a href="https://discord.gg/n8R5VaWDAn"><img src="https://img.shields.io/discord/1287729918100246654?logo=discord&logoColor=white&label=Join+Us&color=blueviolet" alt="Join the goose Discord"></a>
  <a href="https://github.com/aaif-goose/goose/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/aaif-goose/goose/ci.yml?branch=main" alt="CI"></a>
  <a href="https://insights.linuxfoundation.org/project/goose"><img src="https://insights.linuxfoundation.org/api/badge/health-score?project=goose" alt="Project health"></a>
  <a href="https://repology.org/project/goose-cli/versions"><img src="https://repology.org/badge/tiny-repos/goose-cli.svg" alt="Packaging status"></a>
</p>

<a href="https://trendshift.io/repositories/25298?utm_source=repository-badge&amp;utm_medium=badge&amp;utm_campaign=badge-repository-25298" target="_blank" rel="noopener noreferrer"><img src="https://trendshift.io/api/badge/repositories/25298" alt="aaif-goose%2Fgoose | Trendshift" width="250" height="55"/></a>

**[Use goose](#use-goose) · [Build with GDK](#build-with-the-goose-development-kit-gdk) · [Documentation](https://goose-docs.ai/)**

<sub>Part of the <a href="https://aaif.io/">Agentic AI Foundation (AAIF)</a> at the Linux Foundation.</sub>

</div>

## Use goose

goose is a general-purpose AI agent that runs on your machine.
Use it for coding, research, writing, automation, and data analysis through the desktop app or CLI.

- **Choose your models.**
  Connect to Anthropic, OpenAI, Google, Ollama, OpenRouter, Azure, Bedrock, and other providers, or use your existing Claude, ChatGPT, or Gemini subscriptions through [ACP providers](https://goose-docs.ai/docs/guides/acp-providers).
- **Connect your tools.**
  Add capabilities through [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) extensions, from working with files and code to interacting with services and applications.
- **Reuse your workflows.**
  Capture instructions, tools, and parameters in [recipes](https://goose-docs.ai/docs/guides/recipes/recipe-reference), and extend goose with [skills](https://goose-docs.ai/docs/guides/context-engineering/using-skills).

**[Download the desktop app](https://goose-docs.ai/docs/getting-started/installation)** for macOS, Linux, and Windows, or install the CLI:

```bash
curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | bash
```

Follow the [quickstart](https://goose-docs.ai/docs/quickstart) to connect a model and run your first session.
To build a preconfigured goose distribution for your team, see [Custom Distributions](https://github.com/aaif-goose/goose/blob/main/CUSTOM_DISTROS.md).

## Build with the goose Development Kit (GDK)

GDK is the set of reusable Rust components used to build goose.
Use them to build agent behavior into your own applications, with control over models, tools, storage, and execution.
The agent runtime supports native applications, servers, and WebAssembly hosts.

- **Compose your agent.**
  Use [`goose-agent`](https://docs.rs/goose-agent/latest/goose_agent/) to assemble a state machine from operations and model calls, execute typed tools, stream events, and resume from state persisted by your application.
- **Connect to models.**
  Use [`goose-providers`](https://docs.rs/goose-providers/latest/goose_providers/) for hosted services, local model servers, or in-process local inference.
- **Manage context.**
  Use [`goose-context-management`](https://docs.rs/goose-context-management/latest/goose_context_management/) to summarize and compact conversations through your own storage interfaces.

Use the components your application needs, individually or together.
The Rust crates expose the agent runtime; the Python and Kotlin bindings expose provider calls, streaming, and conversation compaction.
GDK is currently in alpha.

**[Getting started](https://goose-docs.ai/docs/gdk/hello-world) · [Demos](https://goose-docs.ai/docs/gdk/demos) · [Concepts](https://goose-docs.ai/docs/gdk/concepts) · [API reference](https://goose-docs.ai/docs/gdk/reference)**

### Use goose as your application's agent

If you want the complete goose agent behind your own interface, connect to it through the [Agent Client Protocol (ACP)](https://goose-docs.ai/docs/gdk/acp).
Your application provides the interface and workflow; goose supplies the agent, configured models, tools, and saved conversations.
This is how goose Desktop connects to its backend.

## Documentation and support

- [Documentation](https://goose-docs.ai/)
- [Tutorials](https://goose-docs.ai/docs/category/tutorials)
- [Diagnostics and reporting](https://goose-docs.ai/docs/troubleshooting/diagnostics-and-reporting)
- [Known issues](https://goose-docs.ai/docs/troubleshooting/known-issues)

## Join the community

Ask questions, share what you build, and help shape goose and GDK.
See the [contribution guide](https://github.com/aaif-goose/goose/blob/main/CONTRIBUTING.md) and [governance](https://github.com/aaif-goose/goose/blob/main/GOVERNANCE.md) to get involved.

[Discord](https://discord.gg/n8R5VaWDAn) · [YouTube](https://www.youtube.com/@goose-oss) · [LinkedIn](https://www.linkedin.com/company/goose-oss) · [Twitter/X](https://x.com/goose_oss)
