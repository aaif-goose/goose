---
title: goose Docs Extension

description: Add goose Docs MCP Server as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';


This tutorial covers how to add the [goose Docs MCP Server](https://github.com/idosal/git-mcp) as a goose extension to enable goose to answer questions about itself.

:::tip Quick Install
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Launch the installer](goose://extension?cmd=npx&arg=mcp-remote&arg=https%3A%2F%2Fblock.gitmcp.io%2Fgoose%2F&id=goose-docs&name=goose%20Docs&description=gitmcp%20for%20goose%20documentation)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  **Command**
  ```sh
  npx mcp-remote https://block.gitmcp.io/goose/
  ```
  </TabItem>
</Tabs>
:::

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="goose-docs"
      extensionName="goose Docs"
      description="GitMCP for goose documentation"
      command="npx"
      args={["mcp-remote", "https://block.gitmcp.io/goose/"]}
    />
 </TabItem>
  <TabItem value="cli" label="goose CLI">
      <CLIExtensionInstructions
        name="goose Docs"
        description="GitMCP for goose documentation"
        command="npx mcp-remote https://block.gitmcp.io/goose/"
        timeout={300}
      />
  </TabItem>
</Tabs>


## Example Usage
GitMCP allows you to convert any Git repository into a goose extension. The goose Docs MCP Server is a great example of this, as it provides a way for goose to answer questions about itself.

Let's ask goose to create a recipe for us! It will use the goose Docs MCP Server to find the relevant information.


### goose Prompt

```
hey goose, I want to create a recipe for automated code review that my development team can use. Can you look at the goose documentation to understand the recipe format, then create a recipe that:


1. Reviews pull requests for:

- Code quality issues and best practices
- Security vulnerabilities
- Missing unit tests
- Performance concerns

2. Should be parameterized so teams can customize:

- Programming language (Python, JavaScript, etc.)
- Minimum test coverage threshold
- Specific coding standards to enforce

3. Should output structured results that can be parsed by CI/CD systems

Please research the recipe format first using the goose Docs extension, then create a complete recipe file with proper YAML structure, parameters, and any needed extensions. 
Make it something a development team could immediately use in their workflow.
```


### goose Output

:::note Desktop

Perfect! I've created a comprehensive automated code review recipe based on the goose documentation research. Here's what I've built for your development team:

#### 📋 Recipe Overview

The **Automated Code Review** recipe (`automated-code-review.yaml`) provides:

#### ✅ **Core Review Areas**
1. **Code Quality & Best Practices** - Architecture, naming, complexity, documentation
2. **Security Vulnerability Assessment** - OWASP Top 10, input validation, auth issues
3. **Test Coverage Analysis** - Missing tests, coverage thresholds, test quality
4. **Performance Concerns** - Bottlenecks, algorithm complexity, resource management
5. **Technical Debt Assessment** - Refactoring needs, maintainability issues

#### 🔧 **Parameterization Features**
- **`programming_language`** - Python, JavaScript, Java, Go, Rust, TypeScript, etc.
- **`min_test_coverage`** - Customizable threshold (default: 80%)
- **`coding_standards`** - PEP8, ESLint, Google Style Guide, company-specific
- **`security_focus`** - Basic, standard, or comprehensive security analysis
- **`performance_analysis`** - Enable/disable performance review
- **`exclude_patterns`** - Skip certain files/directories

#### 📊 **Structured CI/CD Output**
The recipe outputs a comprehensive JSON structure with:
- **Overall quality score** and recommendation (approve/request changes/reject)
- **Categorized issues** with severity levels and specific file locations
- **Security vulnerabilities** with CWE IDs and remediation steps
- **Test coverage metrics** and missing test recommendations
- **Performance concerns** with optimization suggestions
- **Technical debt assessment** with effort estimates
- **CI/CD metadata** for pipeline integration

#### 🚀 **Usage Examples**

#### Basic Usage:
```bash
goose run automated-code-review.yaml \
  --repository_path ./my-project \
  --programming_language Python
```

#### Advanced Usage:
```bash
goose run automated-code-review.yaml \
  --repository_path ./my-project \
  --programming_language JavaScript \
  --min_test_coverage 85 \
  --coding_standards ESLint \
  --security_focus comprehensive \
  --exclude_patterns "*.test.js,dist/*,node_modules/*"
```

#### 🔧 **CI/CD Integration**

The structured JSON output makes it perfect for CI/CD pipelines:

```yaml
# Example GitHub Actions integration
- name: Run Code Review
  run: |
    goose run automated-code-review.yaml \
      --repository_path . \
      --programming_language ${{ matrix.language }} \
      --output review-results.json

- name: Process Results
  run: |
    # Parse JSON results for pipeline decisions
    RECOMMENDATION=$(jq -r '.review_summary.recommendation' review-results.json)
    if [ "$RECOMMENDATION" = "reject" ]; then
      exit 1
    fi
```

#### 📁 **File Structure**

The recipe is saved as `automated-code-review.yaml` in your current directory and includes:
- Proper YAML structure following goose recipe format
- Comprehensive parameter definitions with defaults
- Built-in developer extension for file system access
- Detailed JSON schema for structured output
- Template syntax for parameter substitution

Your development team can immediately start using this recipe by placing it in their project repositories and customizing the parameters for their specific needs!
:::

### Generated Recipe File
<details>
<summary>automated-code-review.yaml</summary>

```yaml
version: "1.0.0"
title: "Automated Code Review"
description: "Comprehensive automated code review for pull requests with customizable standards and structured CI/CD output"

instructions: |
  You are an expert code reviewer with deep knowledge of software engineering best practices, 
  security vulnerabilities, and testing methodologies. Your role is to perform a thorough 
  code review focusing on:
  
  1. Code Quality & Best Practices
  2. Security Vulnerabilities  
  3. Test Coverage Analysis
  4. Performance Concerns
  5. Maintainability Issues
  
  Always provide actionable feedback with specific examples and suggestions for improvement.
  Consider the specified programming language conventions and coding standards.

parameters:
  - key: repository_path
    input_type: string
    requirement: required
    description: "Path to the repository or pull request to review"
  
  - key: programming_language
    input_type: string
    requirement: required
    description: "Primary programming language (e.g., Python, JavaScript, Java, Go, Rust, TypeScript)"
  
  - key: min_test_coverage
    input_type: number
    requirement: optional
    default: 80
    description: "Minimum test coverage threshold percentage (0-100)"
  
  - key: coding_standards
    input_type: string
    requirement: optional
    default: "industry_standard"
    description: "Coding standards to enforce (e.g., PEP8, ESLint, Google Style Guide, company_specific)"
  
  - key: security_focus
    input_type: string
    requirement: optional
    default: "standard"
    description: "Security review depth (basic, standard, comprehensive)"
  
  - key: performance_analysis
    input_type: string
    requirement: optional
    default: "true"
    description: "Enable performance analysis (true/false)"
  
  - key: exclude_patterns
    input_type: string
    requirement: optional
    default: "*.md,*.txt,*.json,node_modules/*,vendor/*,.git/*"
    description: "Comma-separated patterns of files/directories to exclude from review"

extensions:
  - type: builtin
    name: developer
    timeout: 600
    bundled: true
    description: "Access to file system and shell commands for code analysis"

prompt: |
  Perform a comprehensive code review of the {{ programming_language }} codebase at {{ repository_path }}.
  
  ## Review Scope
  - Programming Language: {{ programming_language }}
  - Minimum Test Coverage: {{ min_test_coverage }}%
  - Coding Standards: {{ coding_standards }}
  - Security Focus: {{ security_focus }}
  - Performance Analysis: {{ performance_analysis }}
  - Exclude Patterns: {{ exclude_patterns }}
  
  ## Analysis Requirements
  
  ### 1. Code Quality & Best Practices
  - Review code structure, organization, and architecture
  - Check adherence to {{ coding_standards }} standards
  - Identify code smells, anti-patterns, and technical debt
  - Evaluate naming conventions, documentation, and readability
  - Check for proper error handling and logging
  
  ### 2. Security Vulnerability Assessment
  {% if security_focus == "comprehensive" %}
  Perform comprehensive security analysis including:
  - OWASP Top 10 vulnerabilities
  - Input validation and sanitization
  - Authentication and authorization flaws
  - Cryptographic implementations
  - Dependency vulnerabilities
  - Information disclosure risks
  {% elif security_focus == "standard" %}
  Perform standard security review:
  - Common vulnerability patterns
  - Input validation issues
  - Authentication/authorization problems
  - Basic cryptographic concerns
  {% else %}
  Perform basic security check:
  - Obvious security anti-patterns
  - Hardcoded credentials
  - Basic input validation
  {% endif %}
  
  ### 3. Test Coverage Analysis
  - Analyze existing test files and coverage
  - Identify missing unit tests for critical functions
  - Check for integration and end-to-end test gaps
  - Evaluate test quality and maintainability
  - Verify test coverage meets {{ min_test_coverage }}% threshold
  
  ### 4. Performance Concerns
  {% if performance_analysis == "true" %}
  - Identify potential performance bottlenecks
  - Review algorithm complexity and efficiency
  - Check for memory leaks and resource management
  - Analyze database queries and data access patterns
  - Review caching strategies and optimization opportunities
  {% endif %}
  
  ### 5. Maintainability & Technical Debt
  - Assess code complexity and maintainability
  - Identify areas needing refactoring
  - Check for proper separation of concerns
  - Evaluate dependency management
  - Review documentation completeness
  
  ## Output Requirements
  Provide structured analysis that can be parsed by CI/CD systems, including:
  - Overall quality score and recommendation
  - Categorized issues with severity levels
  - Specific file locations and line numbers
  - Actionable remediation steps
  - Test coverage metrics and missing test recommendations
  - Security findings with risk assessments

response:
  json_schema:
    type: object
    properties:
      review_summary:
        type: object
        properties:
          overall_score:
            type: number
            minimum: 0
            maximum: 100
            description: "Overall code quality score (0-100)"
          recommendation:
            type: string
            enum: ["approve", "approve_with_suggestions", "request_changes", "reject"]
            description: "Review recommendation for CI/CD pipeline"
          total_issues:
            type: number
            description: "Total number of issues found"
          critical_issues:
            type: number
            description: "Number of critical/blocking issues"
        required: ["overall_score", "recommendation", "total_issues", "critical_issues"]
      
      code_quality:
        type: object
        properties:
          score:
            type: number
            minimum: 0
            maximum: 100
            description: "Code quality score"
          issues:
            type: array
            items:
              type: object
              properties:
                severity:
                  type: string
                  enum: ["critical", "major", "minor", "info"]
                category:
                  type: string
                  enum: ["architecture", "naming", "complexity", "documentation", "best_practices"]
                file_path:
                  type: string
                  description: "Relative path to the file"
                line_number:
                  type: number
                  description: "Line number where issue occurs"
                description:
                  type: string
                  description: "Description of the issue"
                suggestion:
                  type: string
                  description: "Suggested fix or improvement"
              required: ["severity", "category", "file_path", "description", "suggestion"]
        required: ["score", "issues"]
      
      security_analysis:
        type: object
        properties:
          risk_score:
            type: number
            minimum: 0
            maximum: 100
            description: "Security risk score (0=low risk, 100=high risk)"
          vulnerabilities:
            type: array
            items:
              type: object
              properties:
                severity:
                  type: string
                  enum: ["critical", "high", "medium", "low"]
                type:
                  type: string
                  description: "Type of vulnerability (e.g., SQL Injection, XSS, etc.)"
                file_path:
                  type: string
                line_number:
                  type: number
                description:
                  type: string
                impact:
                  type: string
                  description: "Potential impact of the vulnerability"
                remediation:
                  type: string
                  description: "Steps to fix the vulnerability"
                cwe_id:
                  type: string
                  description: "Common Weakness Enumeration ID if applicable"
              required: ["severity", "type", "file_path", "description", "impact", "remediation"]
        required: ["risk_score", "vulnerabilities"]
      
      test_coverage:
        type: object
        properties:
          current_coverage:
            type: number
            minimum: 0
            maximum: 100
            description: "Current test coverage percentage"
          meets_threshold:
            type: boolean
            description: "Whether coverage meets the minimum threshold"
          missing_tests:
            type: array
            items:
              type: object
              properties:
                file_path:
                  type: string
                function_name:
                  type: string
                line_number:
                  type: number
                priority:
                  type: string
                  enum: ["critical", "high", "medium", "low"]
                test_type:
                  type: string
                  enum: ["unit", "integration", "end_to_end"]
                description:
                  type: string
                  description: "Description of what should be tested"
              required: ["file_path", "function_name", "priority", "test_type", "description"]
          test_quality_issues:
            type: array
            items:
              type: object
              properties:
                file_path:
                  type: string
                issue:
                  type: string
                suggestion:
                  type: string
              required: ["file_path", "issue", "suggestion"]
        required: ["current_coverage", "meets_threshold", "missing_tests", "test_quality_issues"]
      
      performance_analysis:
        type: object
        properties:
          concerns:
            type: array
            items:
              type: object
              properties:
                severity:
                  type: string
                  enum: ["critical", "major", "minor"]
                type:
                  type: string
                  enum: ["algorithm_complexity", "memory_leak", "database_query", "resource_management", "caching"]
                file_path:
                  type: string
                line_number:
                  type: number
                description:
                  type: string
                impact:
                  type: string
                  description: "Performance impact description"
                optimization:
                  type: string
                  description: "Suggested optimization"
              required: ["severity", "type", "file_path", "description", "impact", "optimization"]
        required: ["concerns"]
      
      technical_debt:
        type: object
        properties:
          debt_score:
            type: number
            minimum: 0
            maximum: 100
            description: "Technical debt score (0=low debt, 100=high debt)"
          refactoring_suggestions:
            type: array
            items:
              type: object
              properties:
                priority:
                  type: string
                  enum: ["critical", "high", "medium", "low"]
                area:
                  type: string
                  enum: ["architecture", "code_duplication", "complexity", "dependencies", "documentation"]
                file_path:
                  type: string
                description:
                  type: string
                effort_estimate:
                  type: string
                  enum: ["small", "medium", "large"]
                business_impact:
                  type: string
              required: ["priority", "area", "file_path", "description", "effort_estimate"]
        required: ["debt_score", "refactoring_suggestions"]
      
      ci_cd_metadata:
        type: object
        properties:
          review_timestamp:
            type: string
            description: "ISO timestamp of when review was performed"
          reviewer:
            type: string
            description: "goose automated code review"
          language:
            type: string
            description: "Programming language reviewed"
          standards_applied:
            type: string
            description: "Coding standards that were applied"
          files_reviewed:
            type: number
            description: "Number of files reviewed"
          lines_of_code:
            type: number
            description: "Total lines of code reviewed"
        required: ["review_timestamp", "reviewer", "language", "standards_applied", "files_reviewed"]
    
    required: [
      "review_summary", 
      "code_quality", 
      "security_analysis", 
      "test_coverage", 
      "performance_analysis", 
      "technical_debt", 
      "ci_cd_metadata"
    ]

```

</details>
