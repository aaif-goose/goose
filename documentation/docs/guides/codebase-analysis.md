---
title: Codebase Analysis
sidebar_label: Codebase Analysis
description: Understand codebases with semantic analysis and call graphs
sidebar_position: 111
---

The [Developer extension](/docs/mcp/developer-mcp) includes an `analyze` tool that helps you understand code structure, track symbol usage, and explore call graphs across your codebase. It's automatically available when the Developer extension is enabled and supports file types for [multiple programming languages](https://github.com/aaif-goose/goose/blob/main/crates/goose-mcp/src/developer/analyze/languages/mod.rs).

<details>
<summary>Example analysis: Tracking a function across files</summary>

**Prompt:**

"Show me the call graph for the authenticate function"

**Output:**

:::info Desktop
I'll analyze how the `authenticate` function is used across your codebase.

Based on my analysis, here's how the `authenticate` function is used in your codebase:

#### Function Definition
The `authenticate` function is defined in `auth.py` at line 15. It takes username and password parameters and handles user authentication.

#### Incoming Calls (Functions that call authenticate)
- **login_handler()** in `api.py:25` - The API endpoint that processes login requests
- **admin_login()** in `admin.py:102` - Special authentication flow for admin users
- **test_auth()** in `tests/test_auth.py:45` - Test case for authentication

#### Outgoing Calls (Functions that authenticate calls)
- **validate_credentials()** in `db.py:102` - Checks credentials against the database
- **log_attempt()** in `logger.py:45` - Records authentication attempts
- **check_rate_limit()** in `security.py:78` - Prevents brute force attacks

This shows that `authenticate` is a central function in your authentication flow, called by multiple entry points and depending on database validation, logging, and security checks.
:::

</details>

## Analysis Modes

The `analyze` tool operates in three modes&mdash;Structure, Semantic, and Focus&mdash;depending on whether you’re analyzing directories, files, or symbols. Invoke it through natural language or direct commands with [parameters](#common-parameters).

### Understanding Project Organization

Get a structural overview of your codebase by analyzing a directory—understand project organization, identify large files, and view codebase metrics.

**Natural language:**
- "Can you analyze the structure of my src/ directory?"
- "Give me an overview of this project's code structure"
- "What's the main entry point of this Python project?"

**Direct commands:**
```bash
# Get overview with default depth (3 levels)
analyze path="src/"

# Get overview limited to 2 subdirectory levels
analyze path="." max_depth=2
```

### Inspecting a File

Get semantic details for a single file—see its functions, classes, and imports to understand structure and find specific implementations.

**Natural language:**
- "What functions are in main.py?"
- "Show me the structure of src/utils.py"

**Direct commands:**
```bash
# Get file details
analyze path="main.py"

# Analyze specific file
analyze path="src/utils.py"
```

### Tracking a Symbol Across Files

Focus on a specific function, class, or method to see where it’s defined and how it’s called across files—useful for refactoring and debugging.

**Natural language:**
- "Trace the dependencies for the authenticate function"
- "Show me the call graph for UserClass"

**Direct commands:**
```bash
# Track function usage
analyze path="src/" focus="authenticate"

# Track with deeper call chains
analyze path="." focus="UserClass" follow_depth=3
```

## Common Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `path` | None (required) | Absolute or relative path to the file or directory to analyze |
| `focus` | None | Name of the symbol to track. For cross-file tracking, `path` must be a directory. |
| `follow_depth` | 2 | How many steps to trace from the focused symbol (0=where defined, 1=immediate callers/callees, 2=their callers/callees, etc.). Used with the `focus` parameter. |
| `max_depth` | 3 | How many subdirectory levels to analyze when `path` is a directory (0=unlimited) |
| `force` | false | Receive full analysis results (otherwise, only a warning message is shown when the results exceed 1000 lines) |

## Best Practices

### Handling Large Outputs

If the analysis results exceed 1000 lines, the tool returns a warning message instead of the analysis. Options for managing large outputs:

- **Use `force=true`** to bypass the warning and see the full output (may consume significant conversation context)
- **Narrow your scope** by analyzing a specific subdirectory or file
- **Reduce depth** with `max_depth=1` or `max_depth=2` for directories
- **Delegate to a [subagent](/docs/guides/subagents)** to analyze and summarize without filling your conversation history, for example: "Use a subagent to analyze the entire src/ directory and summarize the main components"

### Performance Tips

- Start with smaller scopes (specific files or subdirectories) before analyzing entire projects
- Use `max_depth=1` or `max_depth=2` to limit directory traversal depth
- Use [`.gooseignore`](/docs/guides/using-gooseignore) and `.gitignore` files to exclude unnecessary files from analysis (like `node_modules/`, build artifacts, or sensitive files)