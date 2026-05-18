---
sidebar_position: 4
title: Subrecipes For Specialized Tasks
sidebar_label: Subrecipes
description: Learn how a recipe can use subrecipes to perform specific tasks
---

Subrecipes are recipes that are used by another recipe to perform specific tasks. They enable:
- **Multi-step workflows** - Break complex tasks into distinct phases with specialized expertise
- **Reusable components** - Create common tasks that can be used in various workflows

:::warning Experimental Feature
Subrecipes are an experimental feature in active development. Behavior and configuration may change in future releases.
:::

## How Subrecipes Work

The "main recipe" registers its subrecipes in the `sub_recipes` field, which contains the following fields:

- `name`: Unique identifier for the subrecipe, used to generate the tool name
- `path`: File path to the subrecipe file (relative or absolute)
- `values`: (Optional) Pre-configured parameter values that are always passed to the subrecipe

When the main recipe is run, goose generates a tool for each subrecipe that:
- Accepts parameters defined by the subrecipe
- Executes the subrecipe in a separate session with its own context
- Returns output to the main recipe

Sub-recipe sessions run in isolation - they don't share conversation history, memory, or state with the main recipe or other subrecipes. Additionally, subrecipes cannot define their own subrecipes (no nesting allowed).

:::note
Recipes that define `sub_recipes` automatically get the `summon` platform extension injected, so the `delegate` tool is available without needing to list it in `extensions`.
:::

### Parameter Handling

Parameters received by subrecipes can be used in prompts and instructions using `{{ parameter_name }}` syntax. Subrecipes receive parameters in two ways:

1. **Pre-set values**: Fixed parameter values defined in the `values` field are automatically provided and cannot be overridden at runtime
2. **Context-based parameters**: The AI agent can extract parameter values from the conversation context, including results from previous subrecipes

Pre-set values take precedence over context-based parameters. If both the conversation context and `values` field provide the same parameter, the `values` version is used.

:::tip
Use the `indent()` filter to maintain valid YAML format when passing multi-line parameter values to subrecipes, for example: `{{ content | indent(2) }}`. See [Template Support](/docs/guides/recipes/recipe-reference#template-support) for more details.
:::

## Examples

### Sequential Processing

This Code Review Pipeline example shows a main recipe that uses two subrecipes to perform a comprehensive code review:

**Usage:**
```bash
goose run --recipe code-review-pipeline.yaml --params repository_path=/path/to/repo
```

**Main Recipe:**

```yaml
# code-review-pipeline.yaml
version: "1.0.0"
title: "Code Review Pipeline"
description: "Automated code review using subrecipes"
instructions: |
  Perform a code review using the available subrecipe tools.
  Run security analysis first, then code quality analysis.

parameters:
  - key: repository_path
    input_type: string
    requirement: required
    description: "Path to the repository to review"

sub_recipes:
  - name: "security_scan"
    path: "./subrecipes/security-analysis.yaml"
    values:
      scan_level: "comprehensive"
  
  - name: "quality_check"
    path: "./subrecipes/quality-analysis.yaml"

extensions:
  - type: builtin
    name: developer
    timeout: 300
    bundled: true

prompt: |
  Review the code at {{ repository_path }} using the subrecipe tools.
  Run security scan first, then quality analysis.
```

**Subrecipes:**

<details>
  <summary>security_scan</summary>
  ```yaml
  # subrecipes/security-analysis.yaml
  version: "1.0.0"
  title: "Security Scanner"
  description: "Analyze code for security vulnerabilities"
  instructions: |
    You are a security expert. Analyze the provided code for security issues.
    Focus on common vulnerabilities like SQL injection, XSS, and authentication flaws.

  parameters:
    - key: repository_path
      input_type: string
      requirement: required
      description: "Path to the code to analyze"
    
    - key: scan_level
      input_type: string
      requirement: optional
      default: "standard"
      description: "Depth of security scan (basic, standard, comprehensive)"

  extensions:
    - type: builtin
      name: developer
      timeout: 300
      bundled: true

  prompt: |
    Perform a {{ scan_level }} security analysis on the code at {{ repository_path }}.
    Report any security vulnerabilities found with severity levels and recommendations.
  ```
</details>

<details>
  <summary>quality_check</summary>
  ```yaml
  # subrecipes/quality-analysis.yaml
  version: "1.0.0"
  title: "Code Quality Analyzer"
  description: "Analyze code quality and best practices"
  instructions: |
    You are a code quality expert. Review code for maintainability, 
    readability, and adherence to best practices.

  parameters:
    - key: repository_path
      input_type: string
      requirement: required
      description: "Path to the code to analyze"

  extensions:
    - type: builtin
      name: developer
      timeout: 300
      bundled: true

  prompt: |
    Analyze the code quality at {{ repository_path }}.
    Check for code smells, complexity issues, and suggest improvements.
  ```
</details>

:::tip
For faster execution when subrecipes are independent, see [Running Subrecipes In Parallel](/docs/tutorials/subrecipes-in-parallel) to execute multiple subrecipes concurrently.
:::

### Conditional Processing

This Smart Project Analyzer example shows conditional logic that chooses between different subrecipes based on analysis:

**Usage:**
```bash
goose run --recipe smart-analyzer.yaml --params repository_path=/path/to/project
```

**Main Recipe:**

```yaml
# smart-analyzer.yaml
version: "1.0.0"
title: "Smart Project Analyzer"
description: "Analyze project and choose appropriate processing based on type"
instructions: |
  First examine the repository to determine the project type (web app, CLI tool, library, etc.).
  Based on what you find:
  - If it's a web application, use the web_security_audit subrecipe
  - If it's a CLI tool or library, use the api_documentation subrecipe
  Only run one subrecipe based on your analysis.

parameters:
  - key: repository_path
    input_type: string
    requirement: required
    description: "Path to the repository to analyze"

sub_recipes:
  - name: "web_security_audit"
    path: "./subrecipes/web-security.yaml"
    values:
      check_cors: "true"
      check_csrf: "true"
  
  - name: "api_documentation"
    path: "./subrecipes/api-docs.yaml"
    values:
      format: "markdown"

extensions:
  - type: builtin
    name: developer
    timeout: 300
    bundled: true

prompt: |
  Analyze the project at {{ repository_path }} and determine its type.
  Then run the appropriate subrecipe tool based on your findings.
```

**Subrecipes:**

<details>
  <summary>web_security_audit</summary>
  ```yaml
  # subrecipes/web-security.yaml
  version: "1.0.0"
  title: "Web Security Auditor"
  description: "Security audit for web applications"
  instructions: |
    You are a web security specialist. Audit web applications for 
    security vulnerabilities specific to web technologies.

  parameters:
    - key: repository_path
      input_type: string
      requirement: required
      description: "Path to the web application code"
    
    - key: check_cors
      input_type: string
      requirement: optional
      default: "false"
      description: "Whether to check CORS configuration"
    
    - key: check_csrf
      input_type: string
      requirement: optional
      default: "false"
      description: "Whether to check CSRF protection"

  extensions:
    - type: builtin
      name: developer
      timeout: 300
      bundled: true

  prompt: |
    Perform a web security audit on {{ repository_path }}.
    {% if check_cors == "true" %}Check CORS configuration for security issues.{% endif %}
    {% if check_csrf == "true" %}Verify CSRF protection is properly implemented.{% endif %}
    Focus on web-specific vulnerabilities like XSS, authentication flaws, and session management.
  ```
</details>

<details>
  <summary>api_documentation</summary>
  ```yaml
  # subrecipes/api-docs.yaml
  version: "1.0.0"
  title: "API Documentation Generator"
  description: "Generate documentation for APIs and libraries"
  instructions: |
    You are a technical writer specializing in API documentation.
    Create comprehensive documentation for code libraries and APIs.

  parameters:
    - key: repository_path
      input_type: string
      requirement: required
      description: "Path to the code to document"
    
    - key: format
      input_type: string
      requirement: optional
      default: "markdown"
      description: "Output format for documentation (markdown, html, rst)"

  extensions:
    - type: builtin
      name: developer
      timeout: 300
      bundled: true

  prompt: |
    Generate {{ format }} documentation for the code at {{ repository_path }}.
    Include API endpoints, function signatures, usage examples, and installation instructions.
    Focus on making it easy for developers to understand and use this code.
  ```
</details>

### Context-Based Parameter Passing

This Travel Planner example shows how subrecipes can receive parameters from conversation context, including results from previous subrecipes:

**Usage:**
```bash
goose run --recipe travel-planner.yaml
```

**Main Recipe:**

```yaml
# travel-planner.yaml
version: "1.0.0"
title: "Travel Activity Planner"
description: "Get weather data and suggest appropriate activities"
instructions: |
  Plan activities by first getting weather data, then suggesting activities based on conditions.

prompt: |
  Plan activities for Sydney by first getting weather data, then suggesting activities based on the weather conditions we receive.

sub_recipes:
  - name: weather_data
    path: "./subrecipes/weather-data.yaml"
    # No values - location parameter comes from prompt context
  
  - name: activity_suggestions
    path: "./subrecipes/activity-suggestions.yaml"
    # weather_conditions parameter comes from conversation context

extensions:
  - type: builtin
    name: developer
    timeout: 300
    bundled: true
```

**Subrecipes:**

<details>
  <summary>weather_data</summary>
  ```yaml
  # subrecipes/weather-data.yaml
  version: "1.0.0"
  title: "Weather Data Collector"
  description: "Fetch current weather conditions for a location"
  instructions: |
    You are a weather data specialist. Gather current weather information
    including temperature, conditions, and seasonal context.

  parameters:
    - key: location
      input_type: string
      requirement: required
      description: "City or location to get weather data for"

  extensions:
    - type: stdio
      name: weather
      cmd: uvx
      args:
        - mcp_weather@latest
      timeout: 300
      description: "Weather data for trip planning"
    - type: builtin
      name: developer
      timeout: 300
      bundled: true

  prompt: |
    Get the current weather conditions for {{ location }}.
    Include temperature, weather conditions (sunny, rainy, etc.), 
    and any relevant seasonal information.
  ```
</details>

<details>
  <summary>activity_suggestions</summary>
  ```yaml
  # subrecipes/activity-suggestions.yaml
  version: "1.0.0"
  title: "Activity Recommender"
  description: "Suggest activities based on weather conditions"
  instructions: |
    You are a travel expert. Recommend appropriate activities and attractions
    based on current weather conditions.

  parameters:
    - key: weather_conditions
      input_type: string
      requirement: required
      description: "Current weather conditions to base recommendations on"

  extensions:
    - type: builtin
      name: developer
      timeout: 300
      bundled: true

  prompt: |
    Based on these weather conditions: {{ weather_conditions }}, 
    suggest appropriate activities, attractions, and travel tips.
    Include both indoor and outdoor options as relevant.
  ```
</details>

In this example:
- The `weather_data` subrecipe gets the location from the prompt context (the AI extracts "Sydney" from the natural language prompt)
- The `activity_suggestions` subrecipe gets weather conditions from the conversation context (the AI uses the weather results from the first subrecipe)

## Best Practices
- **Single responsibility**: Each subrecipe should have one clear purpose
- **Clear parameters**: Use descriptive names and descriptions
- **Pre-set fixed values**: Use `values` for parameters that don't change
- **Test independently**: Verify subrecipes work alone before combining

:::tip Controlling Subrecipe Execution
Each subrecipe can specify its own `settings.max_turns` value to control execution limits. If not specified, the subrecipe inherits the parent recipe's `max_turns` setting. See [Recipe Settings](/docs/guides/recipes/recipe-reference#settings) for details.

```yaml
# subrecipes/quick-scan.yaml
version: "1.0.0"
title: "Quick Security Scan"
settings:
  max_turns: 10  # Limit this subrecipe to 10 turns
instructions: "Perform a quick security scan"
prompt: "Scan for common vulnerabilities"
```
:::

## Learn More
Check out the [Recipes](/docs/guides/recipes) guide for more docs, tools, and resources to help you master goose recipes.
