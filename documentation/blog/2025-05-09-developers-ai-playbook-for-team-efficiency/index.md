---
title: "Championship Driven Development: Your Team's AI Playbook for Peak Performance"
description: How AI-powered 'plays' can transform your dev team into a high-scoring sports team, streamlining game plans for debugging, changelogs, PRs
authors:
    - ian
---

![blog cover](cdd-playbook.png)

# A Developer's Playbook: Using AI for Team Efficiency

Development teams can operate like sports teams. Each member has a role and a shared playbook helps coordinate efforts. Let's explore how AI-driven "plays" can form a starter "playbook" for your dev team, helping with common technical tasks. You can use recipes with [Goose](/) to leverage the [Model Context Protocol (MCP)](https://modelcontextprotocol.io) to make you more productive.

<!-- truncate -->

---

## Understanding the Modern Development Team's Challenges

* Development teams manage complex systems and tools. They work to deliver software quickly and reliably.
* New developers need to learn the team’s processes and tools. This takes time.
* Ensuring consistent quality across all work requires clear standards, e.g.: a sports team practicing plays over and over to achieve consistent execution.
* Teams often use many tools, from IDEs and version control to CI/CD pipelines and issue trackers.
* Managing these tools and the workflows between them can be complex.

## Benefits of Using an AI Playbook

Using a shared AI playbook provides several benefits for a development team:
* **Faster Onboarding:** New team members can use existing recipes to learn standard procedures and become productive more quickly.
* **Improved Consistency:** Standardized recipes ensure tasks are performed the same way every time, leading to more predictable results.
* **Increased Efficiency:** Automating routine tasks frees developers to focus on more complex problem-solving.
* **Knowledge Sharing:** Recipes can codify team knowledge and best practices, making them accessible to everyone.

As teams adopt AI tools like Goose, the ability to define and share these automated workflows will become increasingly important.


## AI Plays: Standardizing Your Team's Workflows

Goose can help standardize and automate these tasks, by [creating recipes](/docs/guides/recipes/session-recipes). As a developer on your team uses Goose, they can create a recipe that describes how to perform a task, and then share that with the rest of the team. These recipes can be shared and reused, and improved over time, just like a sports team’s playbook.

Recipes are built with an understanding of the workflow you want Goose to help with, and these may involve one or more MCP servers, such as [GitHub](/docs/mcp/github-mcp/) or [PostgreSQL](/docs/mcp/postgres-mcp/). The recipes are designed to be reusable and adaptable, allowing developers to create a library that can be used across different projects.

A shared playbook of AI plays helps everyone on the team perform tasks consistently. It can also reduce the time spent on repetitive work.

## Goose Recipes: The Building Blocks of Your Playbook

For a kitchen-related analogy as an overview, check out [Rizel's](/blog/authors/rizel/) recent blog post, [A Recipe for Success](/blog/2025/05/06/recipe-for-success).

A Goose Recipe can be saved from a current Goose session, or written as a YAML file from scratch. It includes instructions for the AI to follow, a prompt for the AI response, optional parameters with data types, and a list of required extensions.

### Creating a Recipe

If you [create a recipe from a current Goose session](/docs/guides/recipes/session-recipes/#create-recipe), it will prompt you for a name and description and will generate some activities that you can edit, along with instructions that you should review and edit. You will be given a URL that you can share with your team.

To create a recipe from scratch, you can use the Goose CLI to create a new recipe file by using a `/recipe` command in the session. This will create a `recipe.yaml` file in your current directory. To make a custom file you can use `/recipe custom-filename.yaml`. From there, you will add your own instructions and activities.

### Validating the Recipe

Like all good developers who test their code (you DO test your code, right??) you can also validate your Goose recipe in your terminal/shell by running `goose validate recipe-filename.yaml` which will check the syntax and structure of the recipe file.

### Sharing the Recipe

If you're using the Goose Desktop app, creating a recipe will give you a URL that you can share directly with your team.

If you're creating the recipe file in YAML, you can share the file with your team, or you can create a URL for it by running this in your terminal/shell: `goose recipe deeplink recipe-filename.yaml`.

### Using a Recipe

Clicking a shared URL from your team will open Goose and load the recipe in a new session. No data is shared between users, so you don't have to worry about leaking API keys or other sensitive information.

For the CLI, you can run the recipe by using the command `goose run recipe-filename.yaml` in your terminal/shell.

:::info PRO TIP
You can set an environment variable to point to a shared GitHub repo for your team's recipes, and teammates can run the recipes by name:
`export GOOSE_RECIPE_GITHUB_REPO=github-username/repo-name`

Then, to run a recipe: `goose run --recipe <recipe-name>`
:::


## A Starter Pack of AI Plays for Your Team

A "starter pack" of AI plays can address common development workflows. This gives your team a foundation for automating routine tasks. Here are some ideas to get you started about the kinds of tasks you can automate with Goose.

### Play 1: Generating Changelogs

Maintaining changelogs is important for tracking project progress and communicating updates. This task can be time-consuming.
An AI play can automate parts of this process. For example, the "Generate Change Logs from Git Commits" play (based on `recipe.yaml` from the provided files) helps create consistent changelogs.

#### How this Play Works:
1.  **Collect Data:** The AI retrieves commit messages, dates, and issue numbers from a Git repository between specified points.
2.  **Categorize Information:** It organizes commits into categories like Features, Bug Fixes, and Performance Improvements.
3.  **Format Output:** The AI formats this information into a structured changelog document.
4.  **Update File:** It can then insert these formatted notes into your existing `CHANGELOG.md` file.

This play helps ensure changelogs are detailed and consistently formatted, saving developer time.

<details>
  <summary>View Changelog recipe</summary>

```yaml
version: 1.0.0
title: Generate Changelog from Commits
description: Generate a weekly Changelog report from Git Commits
prompt: perform the task to generate change logs from the provided git commits
instructions: |
  Task: Add change logs from Git Commits

  1. Please retrieve all commits between SHA {{start_sha}} and SHA {{end_sha}} (inclusive) from the repository.

  2. For each commit:
    - Extract the commit message
    - Extract the commit date
    - Extract any referenced issue/ticket numbers (patterns like #123, JIRA-456)

  3. Organize the commits into the following categories:
    - Features: New functionality added (commits that mention "feat", "feature", "add", etc.)
    - Bug Fixes: Issues that were resolved (commits with "fix", "bug", "resolve", etc.)
    - Performance Improvements: Optimizations (commits with "perf", "optimize", "performance", etc.)
    - Documentation: Documentation changes (commits with "doc", "readme", etc.)
    - Refactoring: Code restructuring (commits with "refactor", "clean", etc.)
    - Other: Anything that doesn't fit above categories

  4. Format the release notes as follows:
    
    # [Version/Date]
    ## Features
    - [Feature description] - [PR #number](PR link)
    ## Bug Fixes
    - [Bug fix description] - [PR #number](PR link)
    [Continue with other categories...]
    
    Example:
    - Optimized query for monthly sales reports - [PR #123](https://github.com/fake-org/fake-repo/pull/123)

  5. Ensure all commit items have a PR link. If you cannot find it, try again. If you still cannot find it, use the commit sha link instead. For example: [commit sha](commit url)

  6. If commit messages follow conventional commit format (type(scope): message), use the type to categorize and include the scope in the notes as a bug, feature, etc

  7. Ignore merge commits and automated commits (like those from CI systems) unless they contain significant information.

  8. For each category, sort entries by date (newest first).

  9. Look for an existing CHANGELOG.md file and understand its format; create the file if it doesn't exist. Then, output the new changelog content at the top of the file, maintaining the same markdown format, and not changing any existing content.

extensions:
- type: builtin
  name: developer
  display_name: Developer
  timeout: 300
  bundled: true
activities:
- Generate release notes from last week's commits
- Create changelog for version upgrade
- Extract PR-linked changes only
- Categorize commits by conventional commit types
author:
  contact: goose-community
```

</details>


### Play 2: Creating Pull Request Descriptions

Having clear Pull Request (PR) descriptions help reviewers understand changes being made, allowing them to provide better feedback. Writing detailed PRs takes effort.

#### How this Play Works:
1.  **Analyze Changes:** The AI analyzes staged changes and unpushed commits in a local Git repository.
2.  **Identify Change Type:** It determines the nature of the changes (e.g., feature, fix, refactor).
3.  **Generate Description:** It creates a PR description including a summary of changes, technical details, a list of modified files, and potential impacts.
4.  **Suggest Branching/Commits (Optional):** Some plays might also suggest branch names or commit messages based on the code changes.

Using this play helps create consistent and informative PRs. This makes the code review process more efficient.

<details>
  <summary>View PR Generator recipe</summary>

```yaml
version: 1.0.0
title: PR Generator
author:
  contact: goose-community
description: Automatically generate pull request descriptions based on changes in a local git repo
instructions: Your job is to generate descriptive and helpful pull request descriptions without asking for additional information. Generate commit messages and branch names based on the actual code changes.
parameters:
  - key: git_repo_path
    input_type: string
    requirement: first_run
    description: path to the repo you want to create PR for
  - key: push_pr
    input_type: boolean
    requirement: optional
    default: false
    description: whether to push the PR after generating the description
extensions:
    - type: builtin
      name: developer
      display_name: Developer
      timeout: 300
      bundled: true
    - type: builtin
      name: memory
      display_name: Memory
      timeout: 300
      bundled: true
      description: "For storing and retrieving formating preferences that might be present"
prompt: |
  Analyze the staged changes and any unpushed commits in the git repository {{git_repo_path}} to generate a comprehensive pull request description. Work autonomously without requesting additional information.

  Analysis steps:
  1. Get current branch name using `git branch --show-current`
  2. If not on main/master/develop:
     - Check for unpushed commits: `git log @{u}..HEAD` (if upstream exists)
     - Include these commits in the analysis
  3. Check staged changes: `git diff --staged`
  4. Save the staged changes diff for the PR description
  5. Determine the type of change (feature, fix, enhancement, etc.) from the code

  Generate the PR description with:
  1. A clear summary of the changes, including:
     - New staged changes
     - Any unpushed commits (if on a feature branch)
  2. Technical implementation details based on both the diff and unpushed commits
  3. List of modified files and their purpose
  4. Impact analysis (what areas of the codebase are affected)
  5. Testing approach and considerations
  6. Any migration steps or breaking changes
  7. Related issues or dependencies

  Use git commands:
  - `git diff --staged` for staged changes
  - `git log @{u}..HEAD` for unpushed commits
  - `git branch --show-current` for current branch
  - `git status` for staged files
  - `git show` for specific commit details
  - `git rev-parse --abbrev-ref --symbolic-full-name @{u}` to check if branch has upstream

  Format the description in markdown with appropriate sections and code blocks where relevant.

  {% if push_pr %}
  Execute the following steps for pushing:
  1. Determine branch handling:
     - If current branch is main/master/develop or unrelated:
       - Generate branch name from staged changes (e.g., 'feature-add-user-auth')
       - Create and switch to new branch: `git checkout -b [branch-name]`
     - If current branch matches changes:
       - Continue using current branch
       - Note any unpushed commits

  2. Handle commits and push:
     a. If staged changes exist:
        - Create commit using generated message: `git commit -m "[type]: [summary]"`
        - Message should be concise and descriptive of actual changes
     b. Push changes:
        - For existing branches: `git push origin HEAD`
        - For new branches: `git push -u origin HEAD`

  3. Create PR:
     - Use git/gh commands to create PR with generated description
     - Set base branch appropriately
     - Print PR URL after creation

  Branch naming convention:
  - Use kebab-case
  - Prefix with type: feature-, fix-, enhance-, refactor-
  - Keep names concise but descriptive
  - Base on actual code changes

  Commit message format:
  - Start with type: feat, fix, enhance, refactor
  - Followed by concise description
  - Based on actual code changes
  - No body text needed for straightforward changes

  Do not:
  - Ask for confirmation or additional input
  - Create placeholder content
  - Include TODO items
  - Add WIP markers
  {% endif %}
```

</details>

### Other Potential Plays for Your Playbook

Your team can create plays for many other tasks:
* **Debugging Assistance:** A play could guide a developer or an AI through initial steps for diagnosing common issues, by checking specific logs or running predefined commands.
* **Log Analysis:** An AI play can define a standard procedure for querying and summarizing log data to identify problems.
* **Documentation Updates:** A "Readme Bot" could have AI assist in generating or updating project README files.
* **Content Migration:** The "dev guide migration" recipe could provide a structured approach to migrating documentation content, ensuring information is preserved and correctly formatted.

## What kinds of tasks can your team automate?

We'd love for you to share your ideas with us! Share your ideas by creating a recipe and posting it to the [Goose community on Discord](http://discord.gg/goose-oss).



<head>
  <meta property="og:title" content="Championship Driven Development: Your team's AI Playbook for Peak Performance" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2025/05/09/developers-ai-playbook-for-team-efficiency" />
  <meta property="og:description" content="Learn how AI-driven 'plays,' based on Model Context Protocol, can help development teams improve common workflows like changelog generation and pull requests." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/cdd-playbook-69a053588574d8678c2acb92a1b21da6.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Championship Driven Development: Your team's AI Playbook for Peak Performance" />
  <meta name="twitter:description" content="Learn how AI-driven 'plays,' based on Model Context Protocol, can help development teams improve common workflows like changelog generation and pull requests." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/cdd-playbook-69a053588574d8678c2acb92a1b21da6.png" />
  <meta name="keywords" content="AI development; Model Context Protocol; developer productivity; team playbook; AI automation; Goose; software development efficiency; changelogs; pull requests" />
</head>