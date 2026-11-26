# New model check: posting the report

The new-model check runs every day on GitHub (`.github/workflows/new-model-check.yml`). You find the latest run, get its report and any fix PR, and post them in the channel. Follow these steps exactly.

## 1. Find the latest run

```
gh run list -R aaif-goose/goose --workflow new-model-check.yml --branch main -L 1 --json databaseId,status,conclusion,createdAt,url
```

- If there's no run, post "No new model check run found." and stop.
- If the run is still `in_progress` or `queued`, wait for it to finish with `gh run watch <databaseId> -R aaif-goose/goose`, then get its result with `gh run view <databaseId> -R aaif-goose/goose --json conclusion`.

## 2. Get the report

```
D=$(mktemp -d)
gh run download <databaseId> -R aaif-goose/goose -n new-model-check-report -D "$D"
```

The report is `$D/new-model-check-report.md`. If the download fails, post "New model check failed and left no report: <url>" and stop.

## 3. Find the PR

If the report has a line `Fix branch: <branch>`, run:

```
gh pr list -R aaif-goose/goose --head <branch> --state all --json url,state
```

- If it returns a PR, add "Fix PR: <url> (<state>)" to the message, with the state in lowercase, for example "(open)" or "(merged)".
- If it returns nothing, add "goose prepared a fix, but the PR wasn't opened." to the message.

## 4. Post

Post one message: the report as it is, then the PR line from step 3 if there is one, then "Run: <url>". If the run's `conclusion` isn't `success`, start the message with "The run failed, so this report may be incomplete."
