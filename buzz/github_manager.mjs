import {
  existsSync,
  readFileSync,
  renameSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";

export const ISSUE_FIRST_COMMENT_MARKER = "<!-- buzz:issue-first -->";

const openPullRequestLinksQuery = `
query($owner: String!, $name: String!, $endCursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(first: 100, after: $endCursor, states: OPEN) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number
        closingIssuesReferences(first: 100) {
          pageInfo { hasNextPage }
          nodes {
            number
            url
            repository { name owner { login } }
          }
        }
      }
    }
  }
}`;
const pullRequestLinksQuery = `
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      closingIssuesReferences(first: 100) {
        pageInfo { hasNextPage }
        nodes {
          number
          url
          repository { name owner { login } }
        }
      }
    }
  }
}`;

export function issueFirstNotice(repository) {
  return `${ISSUE_FIRST_COMMENT_MARKER}
Thanks for contributing. Goose uses an issue-first workflow. Please open or identify the issue for this work, wait until it reaches **Ready**, and link it from this pull request with \`Closes #ISSUE_NUMBER\`.

See [From Issue to Pull Request](https://github.com/${repository}/blob/main/CONTRIBUTING.md#from-issue-to-pull-request). If no issue is linked and nobody replies here, this pull request will be closed after three days.`;
}

export function getProjectIssues(
  runJson,
  { command, projectNumber, projectOwner, projectLimit, repository },
) {
  const normalizedRepository = repository.toLowerCase();
  const snapshotPath = process.env.BUZZ_PROJECT_SNAPSHOT_FILE;
  if (
    snapshotPath &&
    process.env.BUZZ_REFRESH_PROJECT_SNAPSHOT === "true" &&
    existsSync(snapshotPath)
  ) {
    unlinkSync(snapshotPath);
  }
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const project = readProjectSnapshot(snapshotPath) ||
      runJson(command, [
        "project",
        "item-list",
        String(projectNumber),
        "--owner",
        projectOwner,
        "--limit",
        String(projectLimit),
        "--format",
        "json",
      ]);
    if (!Number.isSafeInteger(project.totalCount) || !Array.isArray(project.items)) {
      throw new Error("GitHub returned an invalid project item list.");
    }
    if (project.totalCount > projectLimit) {
      throw new Error(
        `GitHub reports ${project.totalCount} project items. Raise --project-limit.`,
      );
    }
    if (project.items.length === project.totalCount) {
      writeProjectSnapshot(snapshotPath, project);
      return {
        project,
        byNumber: new Map(
          project.items
            .filter(
              (item) =>
                item.content?.type === "Issue" &&
                item.content.repository?.toLowerCase() === normalizedRepository,
            )
            .map((item) => [item.content.number, item]),
        ),
      };
    }
    if (attempt === 1) {
      throw new Error(
        `Expected ${project.totalCount} project items but received ${project.items.length}.`,
      );
    }
  }
}

function readProjectSnapshot(path) {
  if (!path || !existsSync(path)) {
    return null;
  }
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(`Could not read project snapshot ${path}: ${error.message}`);
  }
}

function writeProjectSnapshot(path, project) {
  if (!path || existsSync(path)) {
    return;
  }
  const temporaryPath = `${path}.${process.pid}.tmp`;
  writeFileSync(temporaryPath, `${JSON.stringify(project)}\n`, { mode: 0o600 });
  renameSync(temporaryPath, path);
}

export function getOpenIssues(runJson, { command, repository }) {
  const pages = runJson(command, [
    "api",
    "--paginate",
    "--slurp",
    `repos/${repository}/issues?state=open&per_page=100`,
  ]);
  if (!Array.isArray(pages) || pages.some((page) => !Array.isArray(page))) {
    throw new Error("GitHub returned an invalid paginated issue response.");
  }
  return pages
    .flat()
    .filter((issue) => !issue.pull_request)
    .map((issue) => ({
      number: issue.number,
      title: issue.title,
      url: issue.html_url,
      repository,
      assignees: (issue.assignees || []).map((assignee) => ({
        login: assignee.login,
      })),
    }));
}

export function getOpenPullRequests(
  runJson,
  { command, repository, limit },
) {
  const pages = runJson(command, [
    "api",
    "--paginate",
    "--slurp",
    `repos/${repository}/pulls?state=open&per_page=100`,
  ]);
  if (!Array.isArray(pages) || pages.some((page) => !Array.isArray(page))) {
    throw new Error("GitHub returned invalid pull request metadata.");
  }
  const pullRequests = pages.flat();
  if (pullRequests.length >= limit) {
    throw new Error(
      `GitHub returned ${pullRequests.length} pull requests at the limit. Raise --limit.`,
    );
  }
  const [owner, name] = repositoryParts(repository);
  const linkPages = runJson(command, [
    "api",
    "graphql",
    "--paginate",
    "--slurp",
    "-F",
    `owner=${owner}`,
    "-F",
    `name=${name}`,
    "-f",
    `query=${openPullRequestLinksQuery}`,
  ]);
  if (!Array.isArray(linkPages)) {
    throw new Error("GitHub returned invalid pull request links.");
  }
  const closingByNumber = new Map(
    linkPages.flatMap((page) => {
      const connection = page.data?.repository?.pullRequests;
      if (!connection || !Array.isArray(connection.nodes)) {
        throw new Error("GitHub returned invalid pull request links.");
      }
      return connection.nodes.map((pullRequest) => [
        pullRequest.number,
        closingIssueNodes(pullRequest.closingIssuesReferences),
      ]);
    }),
  );
  return pullRequests.map((pullRequest) => ({
    number: pullRequest.number,
    title: pullRequest.title,
    url: pullRequest.html_url,
    body: pullRequest.body || "",
    createdAt: pullRequest.created_at,
    updatedAt: pullRequest.updated_at,
    isDraft: Boolean(pullRequest.draft),
    author: {
      login: pullRequest.user?.login || null,
      is_bot: pullRequest.user?.type === "Bot",
    },
    authorAssociation: pullRequest.author_association || "UNKNOWN",
    closingIssuesReferences: closingByNumber.get(pullRequest.number) || [],
    comments: [],
  }));
}

export function getPullRequestClosingIssues(
  runJson,
  { command, repository, number },
) {
  const [owner, name] = repositoryParts(repository);
  const result = runJson(command, [
    "api",
    "graphql",
    "-F",
    `owner=${owner}`,
    "-F",
    `name=${name}`,
    "-F",
    `number=${number}`,
    "-f",
    `query=${pullRequestLinksQuery}`,
  ]);
  const pullRequest = result.data?.repository?.pullRequest;
  if (!pullRequest) {
    throw new Error(`Could not find pull request #${number}.`);
  }
  return closingIssueNodes(pullRequest.closingIssuesReferences);
}

function closingIssueNodes(connection) {
  if (!connection || !Array.isArray(connection.nodes)) {
    throw new Error("GitHub returned invalid closing issue links.");
  }
  if (connection.pageInfo?.hasNextPage) {
    throw new Error("A pull request closes more than 100 issues.");
  }
  return connection.nodes;
}

function repositoryParts(repository) {
  const [owner, name, extra] = repository.split("/");
  if (!owner || !name || extra) {
    throw new Error(`Invalid GitHub repository: ${repository}`);
  }
  return [owner, name];
}

export function getRecentIssues(runJson, { command, repository, count }) {
  const issues = [];
  for (let page = 1; issues.length < count; page += 1) {
    const entries = runJson(command, [
      "api",
      `repos/${repository}/issues?state=all&sort=created&direction=desc&per_page=100&page=${page}`,
    ]);
    if (!Array.isArray(entries)) {
      throw new Error("GitHub returned invalid recent issue data.");
    }
    issues.push(...entries.filter((entry) => !entry.pull_request));
    if (entries.length < 100) {
      break;
    }
  }
  return issues.slice(0, count);
}

export function pullRequestReadyAt(pullRequest, timeline) {
  if (pullRequest.draft) {
    return null;
  }
  const readyEvents = (timeline || [])
    .filter(
      (event) =>
        event?.event === "ready_for_review" &&
        Number.isFinite(Date.parse(event.created_at)),
    )
    .sort((left, right) =>
      right.created_at.localeCompare(left.created_at),
    );
  return readyEvents[0]?.created_at || pullRequest.created_at;
}

export function readyTransitionCoreTeamMember(
  events,
  { projectOwner, projectNumber, coreTeamByGithub },
) {
  const transitions = (events || [])
    .filter(
      (event) =>
        event?.status === "Ready" &&
        event.project?.number === projectNumber &&
        event.project.owner?.login?.toLowerCase() === projectOwner.toLowerCase(),
    )
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt));
  const transition = transitions[0];
  if (!transition) {
    return { transition: null, person: null, reason: "no Ready transition" };
  }
  const login = transition.actor?.login;
  if (transition.wasAutomated || !login) {
    return {
      transition,
      person: null,
      reason: "the Ready transition has no human actor",
    };
  }
  const person = coreTeamByGithub.get(login.toLowerCase()) || null;
  return person
    ? { transition, person, reason: "Ready transition actor" }
    : {
        transition,
        person: null,
        reason: `Ready transition actor @${login} is not in the core team`,
      };
}

export function pullRequestIssueMentions(body, repository) {
  const escapedRepository = repository.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const patterns = [
    new RegExp(
      `https://github\\.com/${escapedRepository}/issues/([1-9]\\d*)`,
      "gi",
    ),
    new RegExp(`${escapedRepository}#([1-9]\\d*)`, "gi"),
    /(?:^|[^\w/])#([1-9]\d*)\b/g,
  ];
  const mentions = new Map();
  for (const pattern of patterns) {
    for (const match of String(body || "").matchAll(pattern)) {
      const number = Number.parseInt(match[1], 10);
      if (!mentions.has(number) || match.index < mentions.get(number)) {
        mentions.set(number, match.index);
      }
    }
  }
  return [...mentions]
    .sort((left, right) => left[1] - right[1])
    .map(([number]) => number);
}

export function pullRequestWork(
  pullRequest,
  {
    repository,
    coreTeamGithub,
    viewerLogin,
    now,
    staleAfterMilliseconds,
  },
) {
  if (pullRequest.author?.login?.toLowerCase() === "dependabot[bot]") {
    return null;
  }

  const repositoryLower = repository.toLowerCase();
  const issueMentions = pullRequestIssueMentions(pullRequest.body, repository);
  const closingIssues = (pullRequest.closingIssuesReferences || [])
    .filter(
      (issue) =>
        `${issue.repository?.owner?.login}/${issue.repository?.name}`.toLowerCase() ===
        repositoryLower,
    )
    .map((issue) => ({ number: issue.number, url: issue.url }));
  const closingIssueNumbers = new Set(
    closingIssues.map((issue) => issue.number),
  );
  const unclosedIssueMentions = issueMentions.filter(
    (number) => !closingIssueNumbers.has(number),
  );
  const author = pullRequest.author || {};
  const authorLogin = author.login || null;
  const authorIsCoreTeam = coreTeamGithub.has(authorLogin?.toLowerCase());
  const authorAssociation = pullRequest.authorAssociation || "NONE";
  const authorIsInternal =
    authorIsCoreTeam ||
    ["OWNER", "MEMBER", "COLLABORATOR", "UNKNOWN"].includes(
      authorAssociation,
    );
  const authorIsBot =
    Boolean(author.is_bot) || authorLogin?.toLowerCase().endsWith("[bot]");
  const eligibleForIssueFirst =
    closingIssues.length === 0 &&
    !pullRequest.isDraft &&
    !authorIsInternal &&
    !authorIsBot;
  if (unclosedIssueMentions.length === 0 && !eligibleForIssueFirst) {
    return null;
  }

  const comments = [...(pullRequest.comments || [])].sort(
    (left, right) => Date.parse(left.createdAt) - Date.parse(right.createdAt),
  );
  const warningComments = comments.filter(
    (comment) =>
      (comment.viewerDidAuthor ||
        comment.author?.login?.toLowerCase() === viewerLogin.toLowerCase()) &&
      comment.body?.includes(ISSUE_FIRST_COMMENT_MARKER),
  );
  const warning = warningComments.at(-1) || null;
  const lastComment = comments.at(-1) || null;
  let warningState = "not-commented";
  if (warning) {
    if (warning.id !== lastComment?.id) {
      warningState = "answered";
    } else if (
      Date.parse(warning.createdAt) <= now - staleAfterMilliseconds
    ) {
      warningState = "stale";
    } else {
      warningState = "waiting";
    }
  }

  return {
    number: pullRequest.number,
    title: pullRequest.title,
    url: pullRequest.url,
    created_at: pullRequest.createdAt,
    updated_at: pullRequest.updatedAt,
    is_draft: Boolean(pullRequest.isDraft),
    author: authorLogin,
    author_association: authorAssociation,
    author_is_core_team: authorIsCoreTeam,
    author_is_internal: authorIsInternal,
    author_is_bot: authorIsBot,
    eligible_for_issue_first: eligibleForIssueFirst,
    issue_mentions: issueMentions,
    unclosed_issue_mentions: unclosedIssueMentions,
    closing_issues: closingIssues,
    warning_state: warningState,
    warning_comment: warning
      ? {
          author: warning.author?.login || null,
          created_at: warning.createdAt,
          url: warning.url,
        }
      : null,
    last_comment: lastComment
      ? {
          author: lastComment.author?.login || null,
          created_at: lastComment.createdAt,
          url: lastComment.url,
        }
      : null,
  };
}

export function selectRecentQueueEntries(messages, count, linksFromMessage) {
  const ignored = messages
    .filter((message) => !Number.isSafeInteger(message.created_at))
    .map((message) => ({
      message_id: message.id || null,
      reason: "invalid-created-at",
    }));
  const allEntries = messages
    .filter((message) => Number.isSafeInteger(message.created_at))
    .sort(
      (left, right) =>
        left.created_at - right.created_at ||
        String(left.id || "").localeCompare(String(right.id || "")),
    )
    .flatMap((message) =>
      linksFromMessage(message).map((link) => ({ message, link })),
    );
  const deferredCount = Math.max(0, allEntries.length - count);
  ignored.push(
    ...allEntries.slice(0, deferredCount).map(({ message, link }) => ({
      message_id: message.id,
      link,
      reason: "outside-recent-window",
    })),
  );
  return {
    entries: allEntries.slice(deferredCount),
    ignored,
  };
}

export function readCoreTeam(path) {
  let document;
  try {
    document = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(`Could not read core team file ${path}: ${error.message}`);
  }

  if (!Array.isArray(document.owners) || !Array.isArray(document.members)) {
    throw new Error(`${path} must contain owners and members arrays.`);
  }

  const parsedPeople = [
    ...document.owners.map((entry) => person(entry, "owner", path)),
    ...document.members.map((entry) => person(entry, "member", path)),
  ];
  const people = parsedPeople.map(({ bots, ...entry }) => entry);
  if (people.length === 0) {
    throw new Error(`${path} has no people.`);
  }

  const byGithub = new Map();
  const byPubkey = new Map();
  for (const entry of people) {
    const github = entry.github.toLowerCase();
    if (byGithub.has(github)) {
      throw new Error(`More than one core team entry uses ${entry.github}.`);
    }
    if (byPubkey.has(entry.pubkey)) {
      throw new Error(`More than one core team entry uses ${entry.pubkey}.`);
    }
    byGithub.set(github, entry);
    byPubkey.set(entry.pubkey, entry);
  }

  return {
    people,
    owners: people.filter((entry) => entry.role === "owner"),
    members: people.filter((entry) => entry.role === "member"),
    byGithub,
    byPubkey,
    botsByPerson: new Map(
      parsedPeople.map((entry) => [entry.pubkey, entry.bots]),
    ),
  };
}

function person(entry, role, path) {
  if (
    !entry ||
    typeof entry.name !== "string" ||
    !entry.name.trim() ||
    typeof entry.github !== "string" ||
    !entry.github.trim() ||
    typeof entry.pubkey !== "string" ||
    !/^[0-9a-f]{64}$/i.test(entry.pubkey) ||
    typeof entry.capacity !== "number" ||
    !Number.isFinite(entry.capacity) ||
    entry.capacity <= 0 ||
    !Array.isArray(entry.interest) ||
    entry.interest.length === 0 ||
    entry.interest.some(
      (interest) => typeof interest !== "string" || !interest.trim(),
    )
  ) {
    throw new Error(
      `Every person in ${path} must have a name, GitHub handle, hexadecimal ` +
        "pubkey, positive capacity, and non-empty interest list.",
    );
  }

  const bots = entry.bots || {};
  if (
    typeof bots !== "object" ||
    Array.isArray(bots) ||
    Object.entries(bots).some(
      ([name, pubkey]) =>
        !name.trim() ||
        typeof pubkey !== "string" ||
        !/^[0-9a-f]{64}$/i.test(pubkey),
    )
  ) {
    throw new Error(
      `Bots for ${JSON.stringify(entry.name)} in ${path} must map names to hexadecimal pubkeys.`,
    );
  }

  return {
    name: entry.name.trim(),
    github: entry.github.trim(),
    pubkey: entry.pubkey.toLowerCase(),
    role,
    capacity: entry.capacity,
    interest: entry.interest.map((interest) => interest.trim()),
    bots: Object.entries(bots).map(([name, pubkey]) => ({
      name: name.trim(),
      pubkey: pubkey.toLowerCase(),
      role: "bot",
    })),
  };
}

export function issueReferenceFromChannel(channel) {
  const description = [channel.about, channel.description]
    .filter((value) => typeof value === "string" && value)
    .join("\n");
  const url = description.match(
    /https:\/\/github\.com\/([^/\s]+)\/([^/\s]+)\/(issues|pull)\/([1-9]\d*)/i,
  );
  if (url) {
    return {
      repository: `${url[1]}/${url[2]}`,
      number: Number.parseInt(url[4], 10),
      kind: url[3].toLowerCase() === "issues" ? "issue" : "pull-request",
      source: "description",
    };
  }

  const name = channel.name || "";
  const legacy = name.match(/^([^\s]+\/[^\s]+)\s+#([1-9]\d*)(?:\s|$)/);
  if (legacy) {
    return {
      repository: legacy[1],
      number: Number.parseInt(legacy[2], 10),
      kind: null,
      source: "legacy-name",
    };
  }

  const canonical = name.match(/^#?([1-9]\d*)(?:\s|$)/);
  return canonical
    ? {
        repository: null,
        number: Number.parseInt(canonical[1], 10),
        kind: null,
        source: "name",
      }
    : null;
}

export function channelMatchesIssue(channel, issue) {
  const reference = issueReferenceFromChannel(channel);
  if (
    !reference ||
    reference.kind === "pull-request" ||
    reference.number !== issue.number
  ) {
    return false;
  }
  return (
    !reference.repository ||
    reference.repository.toLowerCase() === issue.repository.toLowerCase()
  );
}

export function bestMatchingIssueChannels(channels, issue) {
  const matches = channels
    .filter((channel) => channelMatchesIssue(channel, issue))
    .map((channel) => ({
      channel,
      rank: issueReferenceRank(issueReferenceFromChannel(channel)),
    }));
  const bestRank = Math.max(0, ...matches.map((match) => match.rank));
  return matches
    .filter((match) => match.rank === bestRank)
    .map((match) => match.channel);
}

export function issueReferenceRank(reference) {
  if (reference?.source === "description") {
    return 3;
  }
  if (reference?.source === "legacy-name") {
    return 2;
  }
  return reference ? 1 : 0;
}

export function repositoryFromIssueUrl(url) {
  try {
    const parts = new URL(url).pathname.split("/").filter(Boolean);
    return parts.length >= 4 && parts[2] === "issues"
      ? `${parts[0]}/${parts[1]}`
      : null;
  } catch {
    return null;
  }
}

export function coreAssignees(assignees, coreGithubHandles) {
  const handles = new Set(
    [...coreGithubHandles].map((handle) => handle.toLowerCase()),
  );
  return (assignees || [])
    .map((assignee) =>
      typeof assignee === "string" ? assignee : assignee?.login,
    )
    .filter(
      (handle) =>
        typeof handle === "string" && handles.has(handle.toLowerCase()),
    );
}

export function firstHumanCommentAfter(comments, enteredAt) {
  const boundary = Date.parse(enteredAt);
  if (!Number.isFinite(boundary)) {
    throw new Error(`Invalid phase-entry time: ${enteredAt}`);
  }
  return (comments || [])
    .filter(
      (comment) =>
        comment?.user?.type === "User" &&
        Number.isSafeInteger(comment.id) &&
        Number.isFinite(Date.parse(comment.created_at)) &&
        Date.parse(comment.created_at) >= boundary,
    )
    .sort(
      (left, right) =>
        Date.parse(left.created_at) - Date.parse(right.created_at) ||
        left.id - right.id,
    )[0] || null;
}

export function linkedPullRequestForVerification(
  pullRequests,
  repository,
  ignoredUrls = [],
) {
  const ignored = new Set(ignoredUrls);
  const candidates = (pullRequests || []).filter(
    (pullRequest) =>
      pullRequest?.repository?.nameWithOwner?.toLowerCase() ===
        repository.toLowerCase() &&
      pullRequest.state !== "CLOSED" &&
      !ignored.has(pullRequest.url),
  );
  if (candidates.length > 1) {
    throw new Error(
      `More than one pull request is linked for verification: ${candidates
        .map((pullRequest) => pullRequest.url)
        .join(", ")}`,
    );
  }
  return candidates[0] || null;
}
