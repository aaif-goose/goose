---
title: Agentic AI and the MCP Ecosystem
description: A 101 introduction to AI Agents
authors: 
    - angie
---

![blog banner](agentic-ai-with-mcp.png)

It seems like yesterday when we all were wowed by generative AI and specifically the chat interfaces that made interacting with large language models (LLMs) accessible to everyday people.

As amazing as this was, it was only the beginning. The next wave of AI is agentic, meaning AI systems that don't just respond to prompts but take actions, make decisions, and interact with external systems. This is accomplished via **AI agents**.

<!--truncate-->

## What are AI Agents?

When you interact with chatbots that use AI, like ChatGPT, you can ask it how to do something, and it'll provide step-by-step instructions.

For example, if I ran into an error while coding, I could paste the error message into ChatGPT and ask it to help me debug. Because ChatGPT doesn't have access to my codebase, it would speculate on the cause of my error and give me a couple of possible solutions to try. I'd then manually try these proposed solutions and return to inform ChatGPT of the results. We'd continue this back and forth until the error is resolved or I give up.

AI Agents greatly simplify this flow by talking with the LLM on my behalf and taking direct action to fix the problem.

> _**An AI agent is a system that operates autonomously to accomplish a goal.**_

Because AI agents are connected to systems, they can analyze a situation, determine the next action, and execute it without much, if any, human intervention. This capability turns them from passive chatbots into automation assistants.

By using an AI agent, I can simply say "fix the error" and it'll have context about what's wrong and automatically fix the error for me.

## How AI Agents Work with LLMs

LLMs (e.g. GPT-4o, Claude 3.5 Sonnet, Gemini 2.0, etc) provide cognitive abilities to AI agents. Most AI agents will have a chat interface themselves where you type your prompt, and the agent will send this prompt to an LLM. At the same time, the agent will also inform the LLM of what **tools** it has access to.

### Tool Calling

Tools are one of the most important aspects of agentic AI. AI agents are able to execute API calls via **tool calling**.

Let's look at an example:

1. A user sends a prompt to their AI agent: _"Fix the NullPointerException in my UserService.java file."_

2. The agent sends the user request and the list of its available tools to the LLM in a structured format.
```
User Request: "Fix the NullPointerException in my UserService.java file."

Available Tools:
1. read_file(file_path: str) → Returns the contents of the specified file.
2. analyze_code(file_content: str) → Identifies potential errors and suggests fixes.
3. edit_file(file_path: str, modifications: dict) → Applies code changes.
4. run_tests() → Executes the test suite and returns results.
```

3. The LLM analyzes the request and selects the appropriate tools. It determines that it needs to read the file in order to help.
```json
[
  {
    "tool": "read_file",
    "parameters": { "file_path": "UserService.java" }
  }
]
```

4. The agent executes `read_file()` and sends the code to the LLM.
```java
public class UserService {
    private Database db;

    public void getUser(String userId) {
        System.out.println(db.findUserById(userId).getName());
    }
}
```

5. The LLM recognizes the issue (`db` variable is not initialized) and now selects another tool to fix the issue.
```json
[
  {
    "tool": "edit_file",
    "parameters": {
      "file_path": "UserService.java",
      "modifications": {
        "line_number": 3,
        "replacement": "private Database db = new Database();"
      }
    }
  }
]
```

6. The agent executes `edit_file()` to fix `UserService.java` and informs the LLM.

7. The LLM determines that it's a good idea to run tests to confirm the change.
```json
[
  {
    "tool": "run_tests",
    "parameters": {}
  }
]
```

8. The agent executes `run_tests()` and provides the results to the LLM.
```json
{
  "tests_passed": true,
  "failed_tests": []
}
```

9. The LLM generates a final message that the agent then presents to the user.
```
I’ve fixed the NullPointerException in UserService.java by initializing the db variable. All tests have passed. Let me know if you need further modifications!
```

Without tool calling, LLMs would only guess answers based on their training data. But by using tools, agents can directly access the data they need to make accurate decisions and take actions.

It's worth noting that not all agents are the same when it comes to tool access. Most proprietary agents are tightly scoped to a specific LLM and a predefined set of tools, as companies build agents tailored for their own applications.

Other agents, like goose, are more extensible, allowing users to configure it with the LLM of their choice, as well as add tools for various APIs, databases, and even [local environments like IDEs](/docs/mcp/jetbrains-mcp). However, for agents to scale across different tools and systems without requiring custom integrations for each one, they need a standardized way to discover, call, and manage tools. This is exactly what the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/introduction) provides.

## MCP Ecosystem

Traditional AI integrations require custom API calls for every system, making scaling difficult. MCP solves this by providing an open, universal protocol for agents to communicate with external systems dynamically.

With MCP, an agent like goose can:

* connect to any API without a developer writing manual integration code
* integrate with cloud services, dev tools, databases, and enterprise systems
* retrieve and store context to enhance reasoning

At the time of this writing, there are more than [1000 MCP servers](https://www.pulsemcp.com/servers) (systems that expose tools) that any MCP-enabled AI agent like goose can connect to! These MCP servers act as bridges between agents and external systems, enabling access to APIs, databases, and development environments. Some were developed by the official API providers, while the vast majority were developed by community members. Because MCP is an open standard, anyone can build an MCP server for any resource. This greatly increases the possibilities of AI agents!

For example, let's say I want goose to develop a new web app for me in my WebStorm IDE based on a Figma design and then commit the code to a new repo in GitHub. I can add the following MCP Servers as goose extensions to give it all of these capabilities:

* [Figma](/docs/mcp/figma-mcp)
* [JetBrains](/docs/mcp/jetbrains-mcp)
* [GitHub](/docs/mcp/github-mcp)

With this, I can prompt my AI agent in natural language and it'll take care of the work:

> _"Based on the figma design with file ID XYZ, build a web app in WebStorm and commit the code to a new GitHub repo named angiejones/myapp"_

Pretty powerful, right?! 

## Get Started with AI Agents
Hopefully this has provided clear insight into what are AI agents, how they work, and what they can enable for you. [goose](/docs/getting-started/installation) is free and open source and you can add as many [extensions](/docs/getting-started/using-extensions#adding-extensions) as you desire. This is a great way to get started with AI agents and see how they can automate tasks in your workflow to make you more efficient.


<head>
  <meta property="og:title" content="Agentic AI and the MCP Ecosystem" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/02/17/agentic-ai-mcp" />
  <meta property="og:description" content="A 101 introduction to AI Agents" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/agentic-ai-with-mcp-1e3050cc8d8ae7a620440e871ad9f0d2.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Agentic AI and the MCP Ecosystem" />
  <meta name="twitter:description" content="A 101 introduction to AI Agents" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/agentic-ai-with-mcp-1e3050cc8d8ae7a620440e871ad9f0d2.png" />
</head>