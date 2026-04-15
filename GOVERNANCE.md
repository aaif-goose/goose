# goose Technical Governance and Stewardship

Learn about goose's governance structure and how to participate

goose follows a lightweight technical governance model designed to support rapid iteration while maintaining community involvement. This document outlines how the project is organized and how decisions are made.

## Core Values
goose's governance is guided by three fundamental values:

* **Open**: goose is open source, but we go beyond code availability. We plan and build in the open. Our roadmap as well as goose recipes, extensions, and prompts are editable and shareable. Our goal is to make goose the most hackable agent available.
* **Flexible**: we prefer open models – but we don’t restrict ourselves. goose equally supports remotely deployed frontier models as well as local private models, whether open or proprietary.
* **Choice**: We're not bound to any one model, protocol, or stack. goose is built for choice and open standards, adapting to your tools, workflow, and identity as a creator.

## Roles

### Contributors

Anyone in the community who contributes to goose through issues, pull requests, or discussions. Community contributions of all kinds, from code and bug reports to feature requests and discussion participation, help ensure goose evolves in directions that serve real user needs and remains aligned with how people actually use the project.

### Maintainers

Maintainers are trusted community members responsible for key components of goose. They review pull requests, guide contributors, and ensure technical and community health within their domain.

#### Responsibilities

* Drive ongoing improvements within their component area.
* Review contributions and maintain quality.
* Foster community participation.
* Surface strategic or architectural decisions to Core Maintainers when needed.

Maintainers have write access to create branches on the repository but not full administrative rights.

### Core Maintainers

Core Maintainers have broad technical understanding of goose and are responsible for the project's overall direction, technical consistency, and long-term vision.

#### Responsibilities

* Setting the overall technical direction and vision for goose
* Define and uphold goose’s technical direction and principles.
* Resolve disputes escalated by Maintainers.
* Appoint and remove Maintainers.
* Ensure the balance between innovation and stability.
* Steward goose in the best interest of the open community.

Core Maintainers have admin access across all repositories but use standard contribution workflows (e.g., pull requests) for transparency.

## Decision-Making Process

### Day-to-Day Decisions

* Most technical and process decisions are made through consensus in pull requests, GitHub discussions, or Discord.
* Core Maintainers can approve and merge changes quickly when there's clear benefit.
* Significant architectural changes should have discussion in a GitHub issue or discussion before implementation.
* Core Maintainers may step in when disputes arise or when decisions have project-wide impact.

### Dispute Resolution

* If Maintainers cannot reach consensus, the matter is escalated to Core Maintainers.
* Core Maintainers aim for consensus through discussion.
* If no resolution is reached after reasonable discussion, Core Maintainers may hold a simple majority vote to resolve the issue.
* All dispute resolutions should be publicly announced on GitHub and Discord.

This process ensures fairness and transparency while enabling timely decision-making.

### Deadlocks

In the event of a decision deadlock in the process above, goose’s creator, Bradley Axen, steps in as a tie breaker to remove the deadlock and make progress.

### Major Changes

Major architectural or directional changes should:

1. Be proposed as a GitHub issue or discussion.
2. Undergo open community review for at least one week.
3. Require approval from a majority of Core Maintainers.
4. Be publicly announced on GitHub and Discord.

## Selection and Removal of Maintainers

### Principles

* Membership is merit-based and tied to individual contributions, not employer affiliation.
* No term limits, but inactivity may lead to emeritus status.
* All appointments and removals are made transparently.

### Maintainer Nomination Process

1. Nomination by any existing Maintainer or Core Maintainer, based on:
   * Sustained, high-quality contributions.
   * Constructive participation in reviews and discussions.
   * Alignment with the project’s values.
2. Discussion among all Core Maintainers.
3. Approval by majority vote.
4. Public announcement on GitHub and Discord.

### Core Maintainer Appointment

We aim to have between 3 and 7 Core Maintainers at any time. We strive for an odd number of Core Maintainers to minimise the chances of voting deadlocks, but technical excellence of candidates takes precedence over adhering precisely to numbers.

1. Nomination by any existing Core Maintainer, based on:
   * Everything required to be a maintainer
   * Demonstrated leadership and judgement.
   * Long-term commitment to the project’s values
2. Discussion among all Core Maintainers.
3. Approval by majority vote.
4. Public announcement on GitHub and Discord.

### Removal

Maintainers or Core Maintainers may be removed in the following cases:

* Extended inactivity of 3+ months without contribution.
* Actions contrary to the project’s values.
* By their own request.

Removal decisions require a majority vote of Core Maintainers and must be documented publicly. Appeals can be made to the Core Maintainers with supporting rationale.

### Succession Planning

If a Core Maintainer leaves for any reason:

* The remaining Core Maintainers should appoint a replacement within 30 days.
* If Core Maintainer count falls below three, appointment of new Core Maintainers becomes a priority and appointment must happen within 15 days. No major decisions will be made until a new Core Maintainer is appointed.
* If there is no qualified developer available who is willing to serve as a Core Maintainer, the remaining Core Maintainer(s) shall instead create and adopt a plan for recruiting and mentoring a new Core Maintainer.
* Until a replacement is appointed, remaining Core Maintainers continue governance responsibilities

## Communication

### Channels

* **GitHub**: The canonical home for issues, pull requests, and documentation.
* **Discord**: Used for real-time collaboration and informal discussions.

### Transparency

* All technical decisions and governance discussions are to be conducted publicly.
* Meeting notes and key decisions are published openly on GitHub.
* Roadmap and priorities are openly discussed and published on GitHub.
* All proposals and changes to governance must be documented via pull requests.

## Working Practices

### Code Review

#### Following our way of working

* Review AI-generated work carefully: Check for redundant comments, tests that provide little to negative value, outdated patterns, and repeated code.
* Prioritize reviews: Others are waiting, but take time to understand the changes.
* Avoid review shopping: Seek review from those familiar with the code being modified.
* Test thoroughly: Manual and automated E2E testing is essential for larger features; post screenshots or videos for UI changes.

#### Contributing

* Discuss first: For new features or architectural changes, open an issue or discussion.
* Keep PRs focused: Smaller, focused changes are easier to review and merge.
* Write meaningful tests: Tests should guard against real bugs, not just increase coverage.
* Engage with the community: All Maintainers should be active on Discord and on GitHub, and be responsive to other contributors.

#### Release Process

* Regular releases with clear documentation of delivered features.
* Quick bug fixes or security resolutions are cherry-picked to patch releases when needed.
* All releases are tested by multiple Core Maintainers or Maintainers before publication.

## Governance Changes

This governance model may evolve as goose grows. Any proposed modification to this document must:

1. Be proposed through a GitHub issue with rationale.
2. Undergo open community discussion for at least one week.
3. Be approved by a majority of Core Maintainers.
4. Clear communication of changes to the community.
5. Implemented via a pull request to the GOVERNANCE.md file in the main goose repository.

## Current Membership

Core Maintainers and Maintainers are listed in the main goose repository's [MAINTAINERS.md](https://github.com/aaif-goose/goose/blob/main/MAINTAINERS.md) file with their areas of expertise where applicable.

## Summary

### goose's governance prioritizes

* **Speed**: Minimal process to support rapid experimentation
* **Openness**: Transparent decision-making and community involvement
* **Autonomy**: Empowering users and contributors to shape goose
* **Quality**: Thoughtful review while avoiding bureaucracy

We believe this balance enables goose to remain innovative while building a strong, engaged community around the shared goal of creating the most hackable, user-controlled AI agent available.

# General Project Policies

Founded by Block and now stewarded by AAIF (Agentic AI Foundation), goose has been established as a Series of LF Projects, LLC. Policies applicable to goose and participants in the goose project, including guidelines on the usage of trademarks, are located at [https://lfprojects.org/policies/](https://lfprojects.org/policies/).  Governance changes approved as per the provisions of this governance document must also be approved by LF Projects, LLC.

goose participants acknowledge that the copyright in all new contributions will be retained by the copyright holder as independent works of authorship and that no contributor or copyright holder will be required to assign copyrights to the project.
Except as described below, all code and specification contributions to the project must be made using the Apache License, Version 2.0 available at (the “Project License”).

All outbound code and specifications will be made available under the Project License. The Core Maintainers may approve the use of an alternative open license or licenses for inbound or outbound contributions on an exception basis.
All documentation (excluding specifications) will be made available under the Creative Commons Attribution 4.0 International license, available at: https://creativecommons.org/licenses/by/4.0.
