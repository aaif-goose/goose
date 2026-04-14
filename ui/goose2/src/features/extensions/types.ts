export interface StdioExtensionConfig {
  type: "stdio";
  name: string;
  description: string;
  cmd: string;
  args: string[];
  envs?: Record<string, string>;
  env_keys?: string[];
  timeout?: number;
  bundled?: boolean;
  available_tools?: string[];
}

export interface BuiltinExtensionConfig {
  type: "builtin";
  name: string;
  description: string;
  display_name?: string;
  timeout?: number;
  bundled?: boolean;
  available_tools?: string[];
}

export interface StreamableHttpExtensionConfig {
  type: "streamable_http";
  name: string;
  description: string;
  uri: string;
  envs?: Record<string, string>;
  env_keys?: string[];
  headers?: Record<string, string>;
  timeout?: number;
  bundled?: boolean;
  available_tools?: string[];
}

export interface SseExtensionConfig {
  type: "sse";
  name: string;
  description: string;
  uri?: string;
}

export type ExtensionConfig =
  | StdioExtensionConfig
  | BuiltinExtensionConfig
  | StreamableHttpExtensionConfig
  | SseExtensionConfig;

export interface ExtensionEntry extends ExtensionConfig {
  config_key: string;
  enabled: boolean;
}
