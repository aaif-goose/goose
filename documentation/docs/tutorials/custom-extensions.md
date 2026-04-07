---
title: Building Custom Extensions
description: Create your own custom MCP Server to use as a goose extension
---

import { PanelLeft } from 'lucide-react';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Building Custom Extensions with goose


goose allows you to extend its functionality by creating your own custom extensions, which are built as MCP servers. These extensions are compatible with goose because it adheres to the [Model Context Protocol (MCP)][mcp-docs]. MCP is an open protocol that standardizes how applications provide context to LLMs. It enables a consistent way to connect LLMs to various data sources and tools, making it ideal for extending functionality in a structured and interoperable way. 

In this guide, we build an MCP server using the [Python SDK for MCP][mcp-python]. We’ll demonstrate how to create an MCP server that reads Wikipedia articles and converts them to Markdown, integrate it as an extension in goose. You can follow a similar process to develop your own custom extensions for goose.

You can check out other example servers in the [MCP servers repository][mcp-servers]. MCP SDKs are also available for other common languages, such as [TypeScript][mcp-typescript] and [Kotlin][mcp-kotlin].

:::info
goose supports Tools, Resources, and Prompts from the [Model Context Protocol](https://modelcontextprotocol.io/). See [`mcp_client.rs`](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/mcp_client.rs) for the supported protocol version and client capabilities.
:::

---

## Prerequisites

Before you begin, ensure you have the following installed on your system:

- **Python 3.13 or higher** - Required for the MCP server
- **[uv](https://docs.astral.sh/uv/)** - Python package manager used in this tutorial
- **Node.js and npm** - Only required if you want to use the MCP Inspector development tool in [Step 4](#step-4-test-your-mcp-server).

---

## Step 1: Initialize Your Project

The first step is to create a new project using [uv][uv-docs]. We will name our project `mcp-wiki`.

Run the following commands in your terminal to set up a basic structure for your MCP server:

```bash
uv init --lib mcp-wiki
cd mcp-wiki

mkdir -p src/mcp_wiki
touch src/mcp_wiki/server.py
touch src/mcp_wiki/__main__.py
```

Your project directory structure should look like this:

```plaintext
.
├── README.md
├── pyproject.toml
└── src
    └── mcp_wiki
        ├── __init__.py   # Primary CLI entry point
        ├── __main__.py   # To enable running as a Python module
        ├── py.typed      # Indicates the package supports type hints
        └── server.py     # Your MCP server code (tool, resources, prompts)
```

---

## Step 2: Write Your MCP Server Code

In this step, we’ll implement the core functionality of the MCP server. Here is a breakdown of the key components:

1. **`server.py`**: This file holds the main MCP server code. In this example, we define a single tool to read Wikipedia articles. You can add your own custom tools, resources, and prompts here.
2. **`__init__.py`**: This is the primary CLI entry point for your MCP server.
3. **`__main__.py`**: This file allows your MCP server to be executed as a Python module.

Below is the example implementation for the Wikipedia MCP server:

### `server.py`

```python
import requests
from requests.exceptions import RequestException
from bs4 import BeautifulSoup
from html2text import html2text
from urllib.parse import urlparse

from mcp.server.fastmcp import FastMCP
from mcp.shared.exceptions import McpError
from mcp.types import ErrorData, INTERNAL_ERROR, INVALID_PARAMS

mcp = FastMCP("wiki")

@mcp.tool()
def read_wikipedia_article(url: str) -> str:
    """
    Fetch a Wikipedia article at the provided URL, parse its main content,
    convert it to Markdown, and return the resulting text.

    Usage:
        read_wikipedia_article("https://en.wikipedia.org/wiki/Python_(programming_language)")
    """
    try:
        # Validate input
        if not url.startswith("http"):
            raise ValueError("URL must start with http or https.")

        # SSRF protection: only allow Wikipedia domains
        parsed = urlparse(url)
        hostname = parsed.netloc.lower()
        
        # Allow wikipedia.org or *.wikipedia.org subdomains only
        if hostname != 'wikipedia.org' and not hostname.endswith('.wikipedia.org'):
            raise ValueError(f"Only Wikipedia URLs are allowed. Got: {parsed.netloc}")

        # Add User-Agent header to avoid 403 from Wikipedia
        headers = {
            'User-Agent': 'MCP-Wiki/1.0 (Educational purposes; Python requests)'
        }
        response = requests.get(url, headers=headers, timeout=10)
        if response.status_code != 200:
            raise McpError(
                ErrorData(
                    code=INTERNAL_ERROR,
                    message=f"Failed to retrieve the article. HTTP status code: {response.status_code}"
                )
            )

        soup = BeautifulSoup(response.text, "html.parser")
        content_div = soup.find("div", {"id": "mw-content-text"})
        if not content_div:
            raise McpError(
                ErrorData(
                    code=INVALID_PARAMS,
                    message="Could not find the main content on the provided Wikipedia URL."
                )
            )

        # Convert to Markdown
        markdown_text = html2text(str(content_div))
        return markdown_text

    except ValueError as e:
        raise McpError(ErrorData(code=INVALID_PARAMS, message=str(e))) from e
    except RequestException as e:
        raise McpError(ErrorData(code=INTERNAL_ERROR, message=f"Request error: {str(e)}")) from e
    except Exception as e:
        raise McpError(ErrorData(code=INTERNAL_ERROR, message=f"Unexpected error: {str(e)}")) from e
```

### `__init__.py`

```python
import argparse
from .server import mcp

def main():
    """MCP Wiki: Read Wikipedia articles and convert them to Markdown."""
    parser = argparse.ArgumentParser(
        description="Gives you the ability to read Wikipedia articles and convert them to Markdown."
    )
    parser.parse_args()
    mcp.run()

if __name__ == "__main__":
    main()
```

### `__main__.py`

```python
from mcp_wiki import main

main()
```

---

## Step 3: Define Project Configuration

Configure your project using `pyproject.toml`. This configuration defines the CLI script so that the mcp-wiki command is available as a binary. Below is an example configuration:

```toml
[project]
name = "mcp-wiki"
version = "0.1.0"
description = "MCP Server for Wikipedia"
readme = "README.md"
requires-python = ">=3.13"
dependencies = [
    "beautifulsoup4>=4.14.0",
    "html2text>=2025.4.15",
    "mcp[cli]>=1.25.0",
    "requests>=2.32.3",
]

[project.scripts]
mcp-wiki = "mcp_wiki:main"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

---

## Step 4: Test Your MCP Server

Verify that your MCP server is running in the MCP Inspector (a browser-based development tool) or the server CLI.

<Tabs>
  <TabItem value="ui" label="In MCP Inspector" default>
:::info
MCP Inspector requires Node.js and npm installed on your computer. 
:::

1. Set up the project environment:

   ```bash
   uv sync
   ```

2. Activate your virtual environment:

   ```bash
   source .venv/bin/activate
   ```

3. Run your server in development mode: 

   ```bash
   mcp dev src/mcp_wiki/server.py
   ```
   
   MCP Inspector should open automatically in your browser. On first run, you'll be prompted to install `@modelcontextprotocol/inspector`.

4. Test the tool:
   1. Click `Connect` to initialize your MCP server
   2. On the `Tools` tab, click `List Tools` and click the `read_wikipedia_article` tool
   3. Enter `https://en.wikipedia.org/wiki/Bangladesh` for the URL and click `Run Tool` 

[![MCP Inspector UI](../assets/guides/custom-extension-mcp-inspector.png)](../assets/guides/custom-extension-mcp-inspector.png)

  </TabItem>
  <TabItem value="cli" label="In the CLI">
1. Set up the project environment:

   ```bash
   uv sync
   ```

2. Activate your virtual environment:

   ```bash
   source .venv/bin/activate
   ```
   
3. Install your project locally:

   ```bash
   uv pip install .
   ```

4. Verify the CLI:

   ```bash
   mcp-wiki --help
   ```

   You should see output similar to:

   ```plaintext
   ❯ mcp-wiki --help
   usage: mcp-wiki [-h]

   Gives you the ability to read Wikipedia articles and convert them to Markdown.

   options:
     -h, --help  show this help message and exit
   ```
  </TabItem>
</Tabs>

---

## Step 5: Integrate with goose

To add your MCP server as an extension in goose:

1. Build the extension binary:

   ```bash
   uv pip install .
   ```

2. Open goose Desktop and click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
3. Click `Extensions` in the sidebar
4. Set the `Type` to `STDIO`
5. Provide a name and description for your extension
6. In the `Command` field, provide the absolute path to your executable:
   ```plaintext
   uv run /full/path/to/mcp-wiki/.venv/bin/mcp-wiki
   ```

   For example:
   ```plaintext
   uv run /Users/smohammed/Development/mcp/mcp-wiki/.venv/bin/mcp-wiki
   ```

:::tip Rebuild binary after changes
To see any changes you make to your MCP server code after integrating with goose, re-run `uv pip install .` and then restart goose Desktop.
:::

For the purposes of this guide, we'll run the local version. Alternatively, you can publish your package to PyPI. Once published, the server can be run directly using `uvx`. For example:

```
uvx mcp-wiki
```

---

## Step 6: Use Your Extension in goose

Once integrated, you can start using your extension in goose. Open the goose chat interface and call your tool as needed.

You can verify that goose has picked up the tools from your custom extension by asking it "what tools do you have?"

![goose Chat - Ask about tools](../assets/guides/custom-extension-tools.png)

Then, you can try asking questions that require using the extension you added.

![goose Chat - Use custom extension](../assets/guides/custom-extension-chat.png)

🎉 **Congratulations!** You’ve successfully built and integrated a custom MCP server with goose.

---

## Advanced Features for MCP Extensions

goose supports advanced MCP features that can enhance your extensions.

### MCP Sampling: AI-Powered Tools

**[MCP Sampling](/docs/guides/mcp-sampling)** allows your MCP servers to request AI completions from goose's LLM, transforming simple tools into intelligent agents.

**Key Benefits:**
- Your MCP server doesn't need its own OpenAI/Anthropic API key
- Tools can analyze data, provide explanations, and make intelligent decisions
- Enhanced user experience with smarter, more contextual responses
- Secure by design: requests are isolated and attributed automatically

**Getting Started:**
- Use the `sampling/createMessage` method in your MCP server to request AI assistance
- [goose's implementation](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/mcp_client.rs) currently supports text and image content types
- goose automatically advertises sampling capability to all MCP servers

**Use Cases:** Document summarization, smart search filtering, code analysis, data insights

**Learn More:** See the [MCP Specification](https://modelcontextprotocol.io/specification/draft/client/sampling) for technical details.

### MCP Apps: Interactive Extensions

**[MCP Apps](/docs/tutorials/building-mcp-apps)** enable rich, interactive user interfaces instead of text-only responses.

**Key Benefits:**
- Return interactive UI components from your MCP server tools
- Components render securely in isolated sandboxes within goose Desktop
- Real-time user interactions trigger callbacks to your server

**Use Cases:** Interactive forms, data visualizations, booking interfaces, configuration wizards

**Learn More:** [Building MCP Apps Tutorial](/docs/tutorials/building-mcp-apps)

:::note
goose also supports [MCP-UI](/docs/guides/interactive-chat/mcp-ui), but MCP Apps is the recommended path for new extensions.
:::

[mcp-docs]: https://modelcontextprotocol.io/
[mcp-python]: https://github.com/modelcontextprotocol/python-sdk
[mcp-typescript]: https://github.com/modelcontextprotocol/typescript-sdk
[mcp-kotlin]: https://github.com/modelcontextprotocol/kotlin-sdk
[mcp-servers]: https://github.com/modelcontextprotocol/servers
[uv-docs]: https://docs.astral.sh/uv/getting-started/
