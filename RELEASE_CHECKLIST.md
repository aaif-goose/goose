# goose Release Manual Testing Checklist

Download the release builds from this PR. Once a build is ready, the actions bot will post a comment on this PR
with instructions on how to download and sign.

## Use the following script to create a risk assessment and testing plan:
```
./workflow_recipes/release_risk_check/run.sh {{VERSION}}
```

It will generate an analysis report in `/tmp/release_report_final.md` and perform testing is necessary for high risk pr changes.

## Have goose produce a test plan

Open the release candidate desktop app and have goose produce a test plan by pointing it at this PR. Use a prompt like

> Look at the notes in PR <release PR> and the report at `/tmp/release_report_final.md` and investigate potential risks in this release. After familiarizing yourself with the scope of each change, produce a suggested test plan that I should follow before publishing the release.

goose will produce a plan. Follow this plan to finish testing.
