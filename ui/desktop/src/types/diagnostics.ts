export type DiagnosticsLevel = 'summary' | 'full';

export type SystemInfo = {
  app_version: string;
  architecture: string;
  enabled_extensions: string[];
  model?: string | null;
  os: string;
  os_version: string;
  provider?: string | null;
};

export type DiagnosticsConfig = {
  configPath: string;
  configYaml?: string | null;
  truncated: boolean;
};

export type DiagnosticsError = {
  message: string;
  path?: string | null;
};

export type DiagnosticsExtensions = {
  enabled: string[];
};

export type DiagnosticsTextFile = {
  content: string;
  path: string;
  truncated: boolean;
};

export type DiagnosticsLogs = {
  cli: DiagnosticsTextFile[];
  llm: DiagnosticsTextFile[];
};

export type DiagnosticsPrompt = {
  content: string;
  name: string;
};

export type DiagnosticsScheduledRecipe = {
  content: string;
  path: string;
};

export type DiagnosticsReport = {
  config?: DiagnosticsConfig | null;
  errors: DiagnosticsError[];
  extensions: DiagnosticsExtensions;
  generatedAt: string;
  level: DiagnosticsLevel;
  logs: DiagnosticsLogs;
  prompts: DiagnosticsPrompt[];
  schedule?: unknown;
  scheduledRecipes: DiagnosticsScheduledRecipe[];
  schemaVersion: number;
  session?: unknown;
  system: SystemInfo;
};
