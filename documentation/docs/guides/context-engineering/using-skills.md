---
title: Using Skills
sidebar_position: 2
sidebar_label: Using Skills
---

Skills are reusable sets of instructions and resources that teach goose how to perform specific tasks. A skill can range from a simple checklist to a detailed workflow with domain expertise, and can include supporting files like scripts or templates. Example use cases include deployment procedures, code review checklists, and API integration guides.

:::info
This functionality requires the built-in [Summon extension](/docs/mcp/summon-mcp), available in v1.25.0+.
:::

When a session starts, goose adds any skills that it discovers to its instructions. During the session, goose automatically loads a skill when:
- Your request clearly matches a skill's purpose
- You explicitly ask to use a skill, for example:
  - "Use the code-review skill to review this PR"
  - "Follow the new-service skill to set up the auth service"
  - "Apply the deployment skill"

You can also ask goose what skills are available, or use the CLI `/skills` command to list available skills and load one or more by name (e.g. `/skills code-review edge-case-finder`).

:::info Claude Compatibility
goose skills are compatible with Claude Desktop and other [agents that support Agent Skills](https://agentskills.io/home#adoption).
:::

## Skill Locations

Skills can be stored globally or per-project:

1. `~/.agents/skills/` — Global skills, available in all sessions
2. `.agents/skills/` — Project-level skills, scoped to the current project

Place a `SKILL.md` file inside a named subdirectory. For example, a global skill called
`code-review` goes in `~/.agents/skills/code-review/SKILL.md`.

> **Backward compatibility:** goose also discovers skills from `.goose/skills/`,
> `.claude/skills/`, `~/.claude/skills/`, and platform-specific config directories,
> but `agents/skills/` is the recommended standard.

## Creating a Skill

Create a skill when you have a repeatable workflow that involves multiple steps, specialized knowledge, or supporting files.

### Skill File Structure

Each skill lives in its own directory with a `SKILL.md` file:

```
~/.agents/skills/
└── code-review/
    └── SKILL.md
```

A `SKILL.md` file requires YAML frontmatter with `name` and `description`, followed by the skill content:

```markdown
---
name: code-review
description: Comprehensive code review checklist for pull requests
---

# Code Review Checklist

When reviewing code, check each of these areas:

## Functionality
- [ ] Code does what the PR description claims
- [ ] Edge cases are handled
- [ ] Error handling is appropriate

## Code Quality
- [ ] Follows project style guide
- [ ] No hardcoded values that should be configurable
- [ ] Functions are focused and well-named

## Testing
- [ ] New functionality has tests
- [ ] Tests are meaningful, not just for coverage
- [ ] Existing tests still pass

## Security
- [ ] No credentials or secrets in code
- [ ] User input is validated
- [ ] SQL queries are parameterized
```

### Supporting Files

Skills can include supporting files like scripts, templates, or configuration files. Place them in the skill directory:

```
~/.agents/skills/
└── api-setup/
    ├── SKILL.md
    ├── setup.sh
    └── templates/
        └── config.template.json
```

When goose loads the skill, it sees the supporting files and can access them using the [Developer extension's](/docs/mcp/developer-mcp) file tools.

<details>
<summary>Example Skill with Supporting Files</summary>

**SKILL.md:**
```markdown
---
name: api-setup
description: Set up API integration with configuration and helper scripts
---

# API Setup

This skill helps you set up a new API integration with our standard configuration.

## Steps

1. Run `setup.sh <api-name>` to create the integration directory
2. Copy `templates/config.template.json` to your integration directory
3. Update the config with your API credentials
4. Test the connection

## Configuration

The config template includes:
- `api_key`: Your API key (get from the provider's dashboard)
- `endpoint`: API endpoint URL
- `timeout`: Request timeout in seconds (default: 30)

## Verification

After setup, verify:
- [ ] Config file is valid JSON
- [ ] API key is set and not a placeholder
- [ ] Test connection succeeds
```

**setup.sh:**
```bash
#!/bin/bash
API_NAME=$1
mkdir -p "integrations/$API_NAME"
cp templates/config.template.json "integrations/$API_NAME/config.json"
echo "Created integration directory for $API_NAME"
echo "Edit integrations/$API_NAME/config.json with your credentials"
```

**templates/config.template.json:**
```json
{
  "api_key": "YOUR_API_KEY_HERE",
  "endpoint": "https://api.example.com/v1",
  "timeout": 30,
  "retry_attempts": 3
}
```

</details>

## Common Use Case Examples

<details>
<summary>Deployment Workflow</summary>

```markdown
---
name: production-deploy
description: Safe deployment procedure for production environment
---

# Production Deployment

## Pre-deployment
1. Ensure all tests pass
2. Get approval from at least 2 reviewers
3. Notify #deployments channel

## Deploy
1. Create release branch from main
2. Run `npm run build:prod`
3. Deploy to staging, verify, then production
4. Monitor error rates for 30 minutes

## Rollback
If error rate exceeds 1%:
1. Revert to previous deployment
2. Notify #incidents channel
3. Create incident report
```

</details>

<details>
<summary>Testing Strategy</summary>

```markdown
---
name: testing-strategy
description: Guidelines for writing effective tests in this project
---

# Testing Guidelines

## Unit Tests
- Test one thing per test
- Use descriptive test names: `test_user_creation_fails_with_invalid_email`
- Mock external dependencies

## Integration Tests
- Test API endpoints with realistic data
- Verify database state changes
- Clean up test data after each test

## Running Tests
- `npm test` — Run all tests
- `npm test:unit` — Unit tests only
- `npm test:integration` — Integration tests (requires database)
```

</details>

<details>
<summary>API Integration Guide</summary>

````markdown
---
name: square-integration
description: How to integrate with our Square account
---

# Square Integration

## Authentication
- Test key: Use `SQUARE_TEST_KEY` from `.env.test`
- Production key: In 1Password under "Square Production"

## Common Operations

### Create a customer
```javascript
const customer = await squareup.customers.create({
  email: user.email,
  metadata: { userId: user.id }
});
```

### Handle webhooks
Always verify webhook signatures. See `src/webhooks/square.js` for our handler pattern.

## Error Handling
- `card_declined`: Show user-friendly message, suggest different payment method
- `rate_limit`: Implement exponential backoff
- `invalid_request`: Log full error, likely a bug in our code
````

</details>

:::tip Other goose features that support reuse
- [.goosehints](/docs/guides/context-engineering/using-goosehints): Best for general preferences, project context, and repeated instructions like "Always use TypeScript"
- [recipes](/docs/guides/recipes/session-recipes): Shareable configurations that package instructions, prompts, and settings together
:::

## Best Practices

- **Keep skills focused** — One skill per workflow or domain. If a skill is getting long, consider splitting it.
- **Write for clarity** — Skills are instructions for goose. Use clear, direct language and numbered steps.
- **Include verification steps** — Help goose confirm the workflow completed successfully.

## Additional Resources

import ContentCardCarousel from '@site/src/components/ContentCardCarousel';
import skillsvsmcp from '@site/blog/2025-12-22-agent-skills-vs-mcp/skills-vs-mcp.png';

<ContentCardCarousel
  items={[
    {
      type: 'blog',
      title: 'Did Skills Kill MCP?',
      description: 'An overview of Agent Skills vs MCP',
      thumbnailUrl: skillsvsmcp,
      linkUrl: '/blog/2025/12/22/agent-skills-vs-mcp',
      date: '2025-12-22',
      duration: '4 min read'
    }
  ]}
/>
