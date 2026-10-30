import { Octokit } from "@octokit/rest";

const REPO_OWNER = "aaif-goose";
const REPO_NAME = "goose";

let octokit: Octokit | null = null;

function getOctokit(): Octokit {
  if (!octokit) {
    octokit = new Octokit({
      auth: process.env.GITHUB_TOKEN,
    });
  }
  return octokit;
}

export interface GitHubItem {
  number: number;
  title: string;
  state: string;
  isMerged: boolean;
  author: string;
  createdAt: string;
  updatedAt: string;
  labels: string[];
  body: string;
  comments: number;
  url: string;
}

export interface GitHubComment {
  author: string;
  createdAt: string;
  body: string;
  authorAssociation: string;
  url: string;
  truncated: boolean;
}

export async function searchGitHub(
  query: string,
  options: {
    sort?: "created" | "updated" | "comments";
    order?: "asc" | "desc";
    state?: "open" | "closed" | "all";
    limit?: number;
  } = {},
): Promise<GitHubItem[]> {
  const { sort, order = "desc", state = "all", limit = 10 } = options;
  const api = getOctokit();

  const sanitized = query.replace(/\b(?:repo|org|user):\S+/gi, "").trim();
  const q = `repo:${REPO_OWNER}/${REPO_NAME} ${sanitized}${state !== "all" ? ` state:${state}` : ""}`;

  const response = await api.rest.search.issuesAndPullRequests({
    q,
    ...(sort ? { sort, order } : {}),
    per_page: limit,
  });

  return response.data.items.map((item) => ({
    number: item.number,
    title: item.title,
    state: item.state,
    isMerged: !!item.pull_request?.merged_at,
    author: item.user?.login ?? "unknown",
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    labels: item.labels.map((l) =>
      typeof l === "string" ? l : (l.name ?? ""),
    ),
    body: item.body ?? "",
    comments: item.comments,
    url: item.html_url,
  }));
}

export async function getGitHubItem(number: number): Promise<GitHubItem> {
  const api = getOctokit();

  const response = await api.rest.issues.get({
    owner: REPO_OWNER,
    repo: REPO_NAME,
    issue_number: number,
  });

  const item = response.data;
  return {
    number: item.number,
    title: item.title,
    state: item.state,
    isMerged: !!item.pull_request?.merged_at,
    author: item.user?.login ?? "unknown",
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    labels: item.labels.map((l) =>
      typeof l === "string" ? l : (l.name ?? ""),
    ),
    body: item.body ?? "",
    comments: item.comments,
    url: item.html_url,
  };
}

export async function getGitHubItemComments(
  number: number,
  limit = 10,
): Promise<GitHubComment[]> {
  const api = getOctokit();
  const params = {
    owner: REPO_OWNER,
    repo: REPO_NAME,
    issue_number: number,
    per_page: 100,
  };
  const first = await api.rest.issues.listComments(params);
  const lastLink = first.headers.link?.match(/<([^>]+)>;\s*rel="last"/)?.[1];
  const lastPage = lastLink
    ? Number(new URL(lastLink).searchParams.get("page"))
    : 1;
  const last =
    lastPage > 1
      ? await api.rest.issues.listComments({ ...params, page: lastPage })
      : first;
  let comments = last.data;
  if (lastPage > 1 && comments.length < limit) {
    const previous =
      lastPage === 2
        ? first
        : await api.rest.issues.listComments({ ...params, page: lastPage - 1 });
    comments = [...previous.data, ...comments];
  }
  return comments.slice(-limit).map((comment) => ({
    author: comment.user?.login ?? "unknown",
    authorAssociation: comment.author_association,
    createdAt: comment.created_at,
    body: comment.body?.slice(0, 2000) ?? "",
    truncated: (comment.body?.length ?? 0) > 2000,
    url: comment.html_url,
  }));
}

export async function getGitHubRelease(tag?: string) {
  const api = getOctokit();
  const params = { owner: REPO_OWNER, repo: REPO_NAME };
  const { data } = tag
    ? await api.rest.repos.getReleaseByTag({ ...params, tag })
    : await api.rest.repos.getLatestRelease(params);
  return {
    tag: data.tag_name,
    name: data.name,
    publishedAt: data.published_at,
    prerelease: data.prerelease,
    url: data.html_url,
    body: data.body?.slice(0, 12000) ?? "",
    truncated: (data.body?.length ?? 0) > 12000,
  };
}
