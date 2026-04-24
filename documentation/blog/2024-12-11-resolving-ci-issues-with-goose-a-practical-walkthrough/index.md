---
draft: false
title: "Resolving CI Issues with goose: A Practical Walkthrough"
description: "Leverage goose to simplify your CI debugging process, fetch detailed information about failed CI runs & annotations directly from GitHub, and even apply fixes directly."
date: 2024-12-11
authors:
  - dalton
---

![CI](goose-github-ci.png)

Running into Continuous Integration (CI) failures in pull requests can be quite frustrating but they happen very often. In this post, we leverage the GitHub CLI (`gh`) using goose to simplify your CI debugging process, fetch detailed information about failed CI runs and annotations directly from GitHub, and even apply fixes directly.

<!-- truncate -->

:::warning goose Beta Version
This post was written about a beta version of goose and the commands and flow may have changed.
:::


## Getting Started

Before diving in, ensure you have the necessary tools set up.

### 1. Install and Authenticate GitHub CLI (`gh`)

You'll need the [GitHub CLI](https://cli.github.com/) `gh` to enable goose's access to CI check run details.  

```bash
brew install gh
gh auth login
```

Follow the prompts to authenticate your account.


### 2. Configure goose
Ensure goose is configured and ready to interact with your repository and local tools. Specifically, you will need to configure a goose profile with the GitHub toolkit.

Update your `profiles.yaml` file in `~/.config/goose` with the necessary toolkits:

```yaml
my-profile:
  provider: anthropic
  processor: claude-3-5-sonnet
  accelerator: claude-3-5-sonnet
  moderator: truncate
  toolkits:
    - name: developer
      requires: {}
    - name: github
      requires: {}
```
Start a session with the profile:

```bash
goose session start --profile my-profile
```
If the profile’s toolkits are not correctly configured, goose might fail to access `gh` commands.

## Resolving CI Failures with goose
Once everything is set up, we can now use goose to tackle CI issues with the following steps:

### Step 1: Fetch CI Failure Details
Ask goose to retrieve information about the CI failures for a specific PR.

**Example Request:**

```

Can you tell me about the CI failures on PR #11111 in the organization/repo repository? The check run ID is 123456.
```

You can access a check run ID by clicking on a PR’s checks tab and selecting a failure. The ID will be available in the URL.

**goose's Workflow:**

goose will use the `gh` CLI to check run information and provide a detailed summary of the issues. goose uses the GitHub CLI to:

- Fetch details about failed CI runs, including logs and annotations.
- Provide a clear and actionable summary of issues.

**Example Command goose Runs:**

```curl
gh api -H "Accept: application/vnd.github+json" /repos/<owner>/<repo>/check-runs/<check-run-id>/annotations
```

**Example goose Output:**
```mathematica
Here are the details for Check Run ID 123456:

**Name**: CI-Build-All  
**Status**: completed  
**Conclusion**: failure  

**Annotations Summary:**  
1. **ModuleA Test Failure**  
   File: `ModuleATests`  
   Line: 35  
   Error: Mismatched parameter order in function call.  

2. **ModuleB Test Failure**  
   File: `ModuleBTests`  
   Line: 64  
   Error: Unexpected nil value encountered during initialization.  

3. **ModuleC Test Failure**  
   File: `ModuleCTests`  
   Line: 98  
   Error: Type mismatch in returned value. Expected 'String', found 'Int'.  
```
### Step 2: Request Fixes from goose
Once goose provides the failure details, ask it to resolve the issues directly.

Example Request:

```
Can you fix these issues and stage the changes so I can review them? Do not add unnecessary code comments.
```

**What goose Does:**

- Applies fixes based on error messages.
- Stages changes for review.


**Example goose Response:**
```markdown
I have fixed the following files and staged them:

- `ModuleATests`: Corrected the order of parameters in the function call.  
- `ModuleBTests`: Resolved the unexpected nil value encountered during initialization.  
- `ModuleCTests`: Adjusted the type mismatch in the returned value.  

You can now review the staged changes.
```


### Step 3: Review the Changes
Carefully review the staged changes to ensure they address the issues correctly and avoid unintended modifications.

**Things to Check:**

- No unnecessary comments (e.g., // Fix xyz).
- No alterations to unrelated parts of the code.


If needed, clean up or re-stage the changes before committing.

## Benefits of Using goose
Using goose to resolve CI issues streamlines your workflow by:

- identifying issues and applying fixes with minimal manual effort.
- integrating with tools like the GitHub CLI to validate changes.
- handling repetitive CI debugging tasks while you focus on code quality.

goose allows you to resolve CI failures efficiently, ensuring confidence in your workflow while reducing the effort required for debugging and testing.

Try it out, and let goose handle the heavy lifting of CI debugging for you!

<head>
  <meta property="og:title" content="Resolving CI Issues with goose: A Practical Walkthrough" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/2024/12/11/resolving-ci-issues-with-goose-a-practical-walkthrough" />
  <meta property="og:description" content="Leverage goose to simplify your CI debugging process, fetch detailed information about failed CI runs & annotations directly from GitHub, and even apply fixes directly." />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/goose-github-ci-30930008ab57b0aebae15a03c73a12b5.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Resolving CI Issues with goose: A Practical Walkthrough" />
  <meta name="twitter:description" content="Leverage goose to simplify your CI debugging process, fetch detailed information about failed CI runs & annotations directly from GitHub, and even apply fixes directly." />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/goose-github-ci-30930008ab57b0aebae15a03c73a12b5.png" />
</head>
