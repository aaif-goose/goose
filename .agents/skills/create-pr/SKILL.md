---
name: create-pr
description: >-
  Create a GitHub PR from the current branch: handle uncommitted changes, generate
  a summary, and submit via gh CLI. Use when the user says "create PR", "open PR",
  "submit PR", "push PR", or wants to create a pull request.
---

# Create PR

Create a GitHub PR from the current branch: handle uncommitted changes, generate a summary, and submit.

## Step 1: Rebase Reminder

Before doing anything else, remind the user to rebase onto main if they haven't already. Ask if they'd like to proceed or rebase first.

## Step 2: Check for Uncommitted Changes

Run `git status` to check for staged, unstaged, or untracked changes.

- If there are uncommitted changes, show the user what's outstanding and ask if they'd like to commit them before creating the PR.
- If the user says yes, stage the relevant files, draft a concise commit message based on the changes, and commit.
- If there are no uncommitted changes, move on.

## Step 3: Gather Branch Context

Run these commands in parallel to understand the branch:

1. `git log main..HEAD --oneline` to see all commits on this branch.
2. `git diff main..HEAD --stat` to get the list of changed files.
3. `git diff main..HEAD` to understand what changed in each file.
4. `git rev-parse --abbrev-ref HEAD` to get the current branch name.
5. `git status` to check if the branch has been pushed to remote.

## Step 4: Generate PR Title and Summary

**Title:** Generate a concise PR title (under 72 characters) that captures the intent of the change. Use conventional style: lowercase, imperative mood (e.g., "prevent chat list from reordering when renaming sessions").

**Body:** Generate a PR summary with these four sections:

### Section 1: Overview

Start with metadata tags, then a Problem/Solution block:

- `**Category:**` — one of: `new-feature`, `improvement`, `fix`, `infrastructure`
- `**User Impact:**` — one sentence describing what changed from the user's perspective. Write this as a standalone sentence a non-technical stakeholder would understand (e.g., "Users can now create and schedule repeatable tasks directly from the desktop app."). This line is used for project changelogs.
- `**Problem:**` — describe the user-facing confusion, mismatch, or friction this PR addresses.
- `**Solution:**` — explain how the change resolves that UX problem and, if applicable, why the approach was chosen.

Keep Problem + Solution to 2-4 sentences total. Prioritize intent and expected user experience, but include brief high-level implementation rationale when it explains reliability, maintainability, or code quality.

### Section 2: Changes

Wrap this section in a collapsible `<details>` block with the summary "File changes".

Inside, list every changed file. For each file, use the filename as a bold header, then underneath write one or two sentences about what was changed and why. Focus on intent, not implementation details.

Format:
```
<details>
<summary>File changes</summary>

**path/to/file.ts**
What changed and why.

**path/to/other.rs**
What changed and why.

</details>
```

### Section 3: Reproduction Steps

Numbered steps in plain English for how an engineer can see the outcome of this PR. Assume they know how to run the project. Focus on where to look and what they should see.

### Section 4: Screenshots/Demos (for UX changes)

If the PR includes visual changes, include before/after screenshots or a short demo. If there are no visual changes, omit this section entirely.

## Step 5: Push and Create PR

1. Push the branch to remote if it hasn't been pushed yet: `git push -u origin HEAD`
2. Create the PR using `gh pr create` with the generated title and body. Use a HEREDOC for the body to preserve formatting.
3. Output the PR URL as a clickable hyperlink so the user can open it directly.

## Tone

Write from the perspective of a product designer explaining their thinking to engineers. Be clear and concise — just enough to establish intent. They can read the code; your job is to guide their understanding of the "why."
