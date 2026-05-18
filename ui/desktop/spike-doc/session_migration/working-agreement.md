# ACP Session Migration Working Agreement

## Workflow

Work in small, reviewable changes. Do not move to the next slice until the
current slice has been summarized and reviewed.

For each slice or phase:

1. Make the smallest coherent code or doc change.
2. Run the relevant verification for that change.
3. Update `progress.md`.
4. Update the main migration plan or detailed plan docs if the approach changed.
5. Stop and provide a review summary before moving on.

## Review Summary Format

Before asking for review, include:

- Slice or phase name.
- Files changed.
- What changed in each file.
- Verification run and results.
- Known limitations or behavior not proven yet.
- Progress docs updated.
- Suggested next step.

## Review Gate

After each slice, ask for review explicitly:

```text
Please review this slice before I move to the next one.
```

Do not continue to the next slice until the user confirms.

## Progress Tracking

Always update:

- `ui/desktop/spike-doc/session_migration/progress.md`

Also update the main or detailed plan docs when scope, order, risks, or
decisions change:

- `ui/desktop/spike-doc/session_migration/acp-session-migration-plan.md`
- the relevant detailed plan file, such as `02-acp-session-wrapper.md`

## Change Size

Prefer narrow patches:

- one router or wrapper at a time
- adapter support for one or two ACP update types at a time
- hook integration only after supporting wrapper, router, and adapter pieces
  exist
- tests added alongside the behavior they verify
