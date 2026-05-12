// Kept dependency-free so the switchboard lifts cleanly into its own repo.

export interface Env {
  GITHUB_APP_ID: string;
  GITHUB_APP_PRIVATE_KEY: string;
  GITHUB_WEBHOOK_SECRET: string;
  GITHUB_OAUTH_CLIENT_ID: string;
  GITHUB_OAUTH_CLIENT_SECRET: string;
  /** Stores routing records only — never API keys. */
  INSTALL_REGISTRY: KVNamespace;
}

// Stored under key `install:<installation_id>`. Routing only; nothing
// that could compromise the user if leaked.
export interface InstallRecord {
  installationId: number;
  agentId: string;
  tunnelSecret: string;
  tunnelUrl: string;
  registeredAt: string;
}

export interface RegisterRequest {
  installation_id: number;
  oauth_code: string;
  agent_id: string;
  tunnel_secret: string;
  tunnel_url: string;
}

export interface InstallationEvent {
  action: 'created' | 'deleted' | 'suspend' | 'unsuspend' | 'new_permissions_accepted';
  installation: { id: number; account: { login: string } | null };
  repositories?: Array<{ full_name: string; default_branch?: string }>;
}

export interface PullRequestEvent {
  action: string;
  number: number;
  pull_request: {
    head: { ref: string; sha: string };
    base: { ref: string };
    title: string;
    body: string | null;
    html_url: string;
    draft?: boolean;
  };
  repository: { full_name: string; default_branch: string };
  installation: { id: number };
}

export interface IssueCommentEvent {
  action: 'created' | 'edited' | 'deleted';
  comment: { body: string; user: { login: string } };
  issue: {
    number: number;
    pull_request?: { url: string } | null;
  };
  repository: { full_name: string };
  installation: { id: number };
}
