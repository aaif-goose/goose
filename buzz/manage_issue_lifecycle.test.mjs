import assert from "node:assert/strict";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));

test("persists verification work even when a later transition query fails", (context) => {
  const directory = mkdtempSync(join(tmpdir(), "buzz-lifecycle-"));
  context.after(() => rmSync(directory, { recursive: true }));
  const githubManagerHome = join(directory, "github-manager");
  mkdirSync(githubManagerHome, { recursive: true });

  const coreTeamPath = join(directory, "core-team.json");
  writeFileSync(
    coreTeamPath,
    JSON.stringify({
      owners: [],
      members: [
        {
          name: "Alex",
          github: "alexhancock",
          pubkey: "1".repeat(64),
          capacity: 1,
          interest: ["Build and release infrastructure"],
        },
      ],
    }),
  );

  const statePath = join(
    githubManagerHome,
    "issue-lifecycle-aaif-goose-goose-project-1.json",
  );
  writeFileSync(
    statePath,
    JSON.stringify({
      version: 1,
      repository: "aaif-goose/goose",
      project_id: "project-id",
      checked_at: "2026-09-01T00:00:00Z",
      issues: {
        123: stateEntry("Ready", ["alexhancock"], true),
        124: stateEntry("Inbox", [], true),
      },
    }),
  );

  const ghPath = join(directory, "gh");
  writeFileSync(ghPath, fakeGithubCli());
  chmodSync(ghPath, 0o700);

  const result = spawnSync(join(scriptDirectory, "manage_issue_lifecycle"), [], {
    encoding: "utf8",
    env: {
      ...process.env,
      GH_BIN: ghPath,
      GOOSE_BUZZ_HOME: directory,
      BUZZ_CORE_TEAM_FILE: coreTeamPath,
    },
  });

  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.deepEqual(
    report.agent_work.map((work) => ({
      kind: work.kind,
      issue: work.issue.number,
      pull_request: work.pull_request.number,
      assignee: work.assignee,
    })),
    [
      {
        kind: "verification",
        issue: 123,
        pull_request: 456,
        assignee: "alexhancock",
      },
    ],
  );
  assert.equal(report.failures.length, 1);
  assert.equal(report.failures[0].issue, 124);
  assert.match(report.failures[0].reason, /quota exhausted$/);

  const state = JSON.parse(readFileSync(statePath, "utf8"));
  assert.equal(state.version, 2);
  assert.equal(
    state.issues["123"].verification_pending.assignee,
    "alexhancock",
  );
  assert.equal(state.issues["124"].phase, "Inbox");
});

test("defers before reading the project when GraphQL is low", (context) => {
  const directory = mkdtempSync(join(tmpdir(), "buzz-lifecycle-rate-"));
  context.after(() => rmSync(directory, { recursive: true }));
  const coreTeamPath = join(directory, "core-team.json");
  writeFileSync(
    coreTeamPath,
    JSON.stringify({
      owners: [],
      members: [
        {
          name: "Alex",
          github: "alexhancock",
          pubkey: "1".repeat(64),
          capacity: 1,
          interest: ["Build and release infrastructure"],
        },
      ],
    }),
  );
  const ghPath = join(directory, "gh");
  writeFileSync(
    ghPath,
    `#!/usr/bin/env node
const args = process.argv.slice(2);
if (args[0] === "api" && args[1] === "rate_limit") {
  process.stdout.write(JSON.stringify({
    resources: { graphql: { remaining: 10, reset: 1790000000 } }
  }));
} else {
  process.stderr.write("project access should not happen");
  process.exit(1);
}
`,
  );
  chmodSync(ghPath, 0o700);

  const result = spawnSync(join(scriptDirectory, "manage_issue_lifecycle"), [], {
    encoding: "utf8",
    env: {
      ...process.env,
      GH_BIN: ghPath,
      GOOSE_BUZZ_HOME: directory,
      BUZZ_CORE_TEAM_FILE: coreTeamPath,
    },
  });
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, "deferred-rate-limit");
  assert.equal(report.graphql.remaining, 10);
});

function stateEntry(phase, previousShepherds, legacy = false) {
  const entry = {
    phase,
    phase_updated_at: "2026-09-01T00:00:00Z",
    previous_shepherds: previousShepherds,
    unassignment_pending: [],
    design_comment_id: null,
    design_assignment_complete: false,
    ignored_pull_requests: [],
  };
  if (!legacy) {
    Object.assign(entry, {
      ready_actor: null,
      ready_actor_checked: false,
      ready_actor_reason: null,
      verification_pending: null,
    });
  }
  return entry;
}

function fakeGithubCli() {
  return `#!/usr/bin/env node
const args = process.argv.slice(2);
const output = (value) => process.stdout.write(JSON.stringify(value));

if (args[0] === "api" && args[1] === "rate_limit") {
  output({ resources: { graphql: { remaining: 5000, reset: 1790000000 } } });
} else if (args[0] === "project" && args[1] === "view") {
  output({ id: "project-id", title: "Goose Issues" });
} else if (args[0] === "project" && args[1] === "field-list") {
  output({
    totalCount: 1,
    fields: [
      {
        id: "status-field-id",
        name: "Status",
        type: "ProjectV2SingleSelectField",
        options: [{ id: "verification-option-id", name: "Verification" }]
      }
    ]
  });
} else if (args[0] === "project" && args[1] === "item-list") {
  output({
    totalCount: 2,
    items: [
      {
        id: "item-123",
        status: "Ready",
        assignees: [],
        "linked pull requests": ["https://github.com/aaif-goose/goose/pull/456"],
        content: {
          type: "Issue",
          repository: "aaif-goose/goose",
          number: 123,
          title: "Pin actions",
          url: "https://github.com/aaif-goose/goose/issues/123"
        }
      },
      {
        id: "item-124",
        status: "Ready",
        assignees: ["alexhancock"],
        "linked pull requests": [],
        content: {
          type: "Issue",
          repository: "aaif-goose/goose",
          number: 124,
          title: "Later issue",
          url: "https://github.com/aaif-goose/goose/issues/124"
        }
      }
    ]
  });
} else if (args[0] === "api" && args[1] === "graphql") {
  const number = args.find((argument) => argument.startsWith("number="));
  if (number === "number=124") {
    process.stderr.write("quota exhausted");
    process.exit(1);
  }
  output({
    data: {
      repository: {
        issue: {
          timelineItems: {
            pageInfo: { hasPreviousPage: false },
            nodes: [
              {
                actor: { login: "alexhancock" },
                createdAt: "2026-09-01T00:00:00Z",
                project: { number: 1, owner: { login: "aaif-goose" } },
                status: "Ready",
                wasAutomated: false
              }
            ]
          }
        }
      }
    }
  });
} else if (args[0] === "api" && args.includes("repos/aaif-goose/goose/pulls/456")) {
  output({
    number: 456,
    title: "Pin actions",
    html_url: "https://github.com/aaif-goose/goose/pull/456",
    state: "open",
    merged_at: null,
    draft: false,
    created_at: "2026-08-01T00:00:00Z",
    assignees: []
  });
} else if (args[0] === "api" && args.some((argument) => argument.includes("issues/456/timeline"))) {
  output([[]]);
} else if (args[0] === "api" && args.includes("repos/aaif-goose/goose/issues/123")) {
  output({
    number: 123,
    title: "Pin actions",
    html_url: "https://github.com/aaif-goose/goose/issues/123",
    state: "open",
    assignees: []
  });
} else {
  process.stderr.write("unexpected gh call: " + args.join(" "));
  process.exit(1);
}
`;
}
