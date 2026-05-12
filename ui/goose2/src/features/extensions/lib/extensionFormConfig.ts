import type {
  ExtensionConfig,
  ExtensionEntry,
  StdioExtensionConfig,
  StreamableHttpExtensionConfig,
} from "../types";

export type ExtensionModalType = "stdio" | "streamable_http" | "unsupported";

export interface ExtensionEnvRow {
  key: string;
  value: string;
}

interface ExtensionSubmitConfigInput {
  type: ExtensionModalType;
  name: string;
  description: string;
  cmd: string;
  args: string;
  uri: string;
  timeout: string;
  envVars: ExtensionEnvRow[];
  extension?: ExtensionEntry;
}

type PreservedCommonFields = {
  available_tools?: string[];
  bundled?: boolean;
};

export function parseExtensionEnvRows(
  envs?: Record<string, string>,
  envKeys?: string[],
): ExtensionEnvRow[] {
  const rows: ExtensionEnvRow[] = [];
  const seenKeys = new Set<string>();

  for (const [key, value] of Object.entries(envs ?? {})) {
    rows.push({ key, value });
    seenKeys.add(key);
  }

  for (const key of envKeys ?? []) {
    if (seenKeys.has(key)) continue;
    rows.push({ key, value: "" });
  }

  return rows;
}

export function buildExtensionEnvConfig(
  vars: ExtensionEnvRow[],
): Pick<
  StdioExtensionConfig | StreamableHttpExtensionConfig,
  "envs" | "env_keys"
> {
  const envs: Record<string, string> = {};
  const envKeys: string[] = [];

  for (const v of vars) {
    const key = v.key.trim();
    if (!key) continue;

    if (v.value.trim().length > 0) envs[key] = v.value;
    else envKeys.push(key);
  }

  return {
    ...(Object.keys(envs).length > 0 ? { envs } : {}),
    ...(envKeys.length > 0 ? { env_keys: envKeys } : {}),
  };
}

export function isValidStreamableHttpUri(value: string): boolean {
  try {
    const url = new URL(value.trim());
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export function canSubmitExtensionConfig(input: {
  type: ExtensionModalType;
  name: string;
  cmd: string;
  uri: string;
}): boolean {
  return (
    input.type !== "unsupported" &&
    input.name.trim().length > 0 &&
    (input.type === "stdio"
      ? input.cmd.trim().length > 0
      : isValidStreamableHttpUri(input.uri))
  );
}

function preservedCommonFields(
  extension?: ExtensionEntry,
): PreservedCommonFields {
  return {
    ...(extension &&
    "available_tools" in extension &&
    extension.available_tools?.length
      ? { available_tools: extension.available_tools }
      : {}),
    ...(extension && "bundled" in extension && extension.bundled !== undefined
      ? { bundled: extension.bundled }
      : {}),
  };
}

export function buildExtensionSubmitPayload({
  type,
  name,
  description,
  cmd,
  args,
  uri,
  timeout,
  envVars,
  extension,
}: ExtensionSubmitConfigInput): {
  name: string;
  config: ExtensionConfig;
} | null {
  if (!canSubmitExtensionConfig({ type, name, cmd, uri })) return null;

  const trimmedName = name.trim();
  const envConfig = buildExtensionEnvConfig(envVars);
  const timeoutNum = Number.parseInt(timeout, 10) || 300;
  const commonFields = preservedCommonFields(extension);

  if (type === "stdio") {
    return {
      name: trimmedName,
      config: {
        type: "stdio",
        name: trimmedName,
        description,
        cmd: cmd.trim(),
        args: args
          .split("\n")
          .map((arg) => arg.trim())
          .filter(Boolean),
        ...envConfig,
        timeout: timeoutNum,
        ...commonFields,
      },
    };
  }

  return {
    name: trimmedName,
    config: {
      type: "streamable_http",
      name: trimmedName,
      description,
      uri: uri.trim(),
      ...envConfig,
      ...(extension?.type === "streamable_http" && extension.headers
        ? { headers: extension.headers }
        : {}),
      ...(extension?.type === "streamable_http" && extension.socket
        ? { socket: extension.socket }
        : {}),
      timeout: timeoutNum,
      ...commonFields,
    },
  };
}
