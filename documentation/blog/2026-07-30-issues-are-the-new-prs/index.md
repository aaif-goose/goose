---
title: "Moving to issues as the new PRs"
description: "We’re changing how people contribute to goose by moving design and discussion into issues before agents turn ready issues into code."
authors:
  - douwe
featured: true
image: /img/blog/issues-are-the-new-prs.png
---

![The goose GitHub repository showing 184 open pull requests, with the list fading into a blur](/img/blog/issues-are-the-new-prs.png)

In the olden days, [contributing your first PR to an open source project](https://opensource.guide/how-to-contribute/) was rather involved. Even with great instructions, getting the project to build, the app to run and the tests to pass took real work. And even if you knew exactly which bug to fix, you needed a rough understanding of the project’s architecture and more detailed knowledge of the code you were changing.

[Coding agents](https://block.xyz/inside/block-open-source-introduces-codename-goose) have changed all this. The problem is not code quality. Complaints about AI slop are all over the internet, but code written by agents is often better than what you would get from a first-time contributor. The problem is that agents have [changed the economics of open source](https://github.blog/open-source/maintainers/how-pull-request-limits-are-cutting-down-the-noise/).

<!-- truncate -->

A PR used to be imperfect evidence that someone had done their homework. Why would you painstakingly implement a feature unless you had some reason to believe the maintainers would want it? The work required to fix a bug also forced you to understand the surrounding code. That did not guarantee good judgment, but the friction selected for commitment and forced contributors to acquire context before writing code.

Today, [producing a plausible feature implementation or a bug fix is cheap](https://github.blog/ai-and-ml/github-copilot/assigning-and-completing-issues-with-coding-agent-in-github-copilot/). Deciding whether the change belongs in the project and what the right solution looks like remains expensive. A PR arrives with those questions apparently answered, but often the agent has merely made a series of plausible choices. The maintainer must uncover those choices, decide which ones are right and explain how the others should change. The contributor’s agent will rapidly revise the code, but the maintainer has supplied the judgment. At that point, couldn’t the maintainer simply run their own agent?

Taken to its conclusion, this would leave open source projects with public code but no meaningful public participation: projects developed by a small core team and its agents, while everyone else becomes a passive user or ticket submitter. But the [value of open source](https://github.com/aaif-goose/goose/blob/main/GOVERNANCE.md#contributors) has never been limited to the code outsiders write. Outsiders bring fresh eyes, new ideas, unexpected use cases and domain knowledge. They encounter problems the core team never sees and question assumptions the core team no longer notices.

To preserve that value, we have to let go of the PR as the main contribution mechanism and move outside contribution upstream. The valuable place to contribute is no longer primarily in writing the code, but in filing good issues and, more importantly, taking part in the discussion that turns those issues into well-considered solutions.

We’re moving goose to this model with a public [Goose Issues board](https://github.com/orgs/aaif-goose/projects/1). As before, if you find a bug or want a new feature, you [file a GitHub issue](https://github.com/aaif-goose/goose/issues/new/choose). New issues enter the Inbox. During triage we may ask for more information or close the issue with an explanation. Problems we want to solve move to Accepted / design, where contributors and the core team work out the intended design, architectural constraints and how the result will be verified.

When that discussion has settled, the issue moves to Ready. At that point an agent can write the code. The issue moves to In progress while implementation is underway and Verification when the result is ready for a human to confirm that it works. Only then is it Done. PRs that do not implement a ready issue will be closed, as will PRs where [feedback is left unaddressed](https://github.com/aaif-goose/goose/blob/main/CONTRIBUTING.md#ai-code-reviews).

This also changes who deserves credit. Reporting a problem, reproducing it, contributing domain knowledge, shaping the design, implementing the solution and verifying the result can all be meaningful work. Substantial contributors at each stage should be recognised as [co-authors](https://docs.github.com/en/pull-requests/committing-changes-to-your-project/creating-and-editing-commits/creating-a-commit-with-multiple-authors). The unit of open source contribution is no longer the patch; it is taking a problem all the way to a verified solution. Most of that contribution now happens in the issue.
