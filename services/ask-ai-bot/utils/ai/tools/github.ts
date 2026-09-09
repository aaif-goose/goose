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

  return response.data.items.map((item: any) => ({
    number: item.number,
    title: item.title,
    state: item.state,
    isMerged: !!item.pull_request?.merged_at,
    author: item.user?.login ?? "unknown",
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    labels: item.labels.map((l: any) =>
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

  const item: any = response.data;
  return {
    number: item.number,
    title: item.title,
    state: item.state,
    isMerged: !!item.pull_request?.merged_at,
    author: item.user?.login ?? "unknown",
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    labels: item.labels.map((l: any) =>
      typeof l === "string" ? l : (l.name ?? ""),
    ),
    body: item.body ?? "",
    comments: item.comments,
    url: item.html_url,
  };
}

export async function getGitHubItemComments(
  number: number,
  limit: number = 30,
): Promise<GitHubComment[]> {
  const api = getOctokit();
  const perPage = Math.min(limit, 100);
  const comments: GitHubComment[] = [];

  for (let page = 1; comments.length < limit; page++) {
    const response = await api.rest.issues.listComments({
      owner: REPO_OWNER,
      repo: REPO_NAME,
      issue_number: number,
      per_page: perPage,
      page,
    });

    if (response.data.length === 0) break;

    for (const comment of response.data) {
      comments.push({
        author: comment.user?.login ?? "unknown",
        createdAt: comment.created_at,
        body: comment.body ?? "",
      });
      if (comments.length >= limit) break;
    }
  }

  return comments;
}
