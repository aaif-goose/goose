import { useState } from "react";
import type { ExtensionConfig, ExtensionEntry } from "../types";

export type ExtensionModalType = "stdio" | "streamable_http" | "unsupported";

export interface EnvVar {
  id: number;
  key: string;
  value: string;
}

let nextEnvId = 0;

function newEmptyEnvVar(): EnvVar {
  return { id: nextEnvId++, key: "", value: "" };
}

function parseEnvVars(envs?: Record<string, string>): EnvVar[] {
  if (!envs || Object.keys(envs).length === 0) return [newEmptyEnvVar()];
  return Object.entries(envs).map(([key, value]) => ({
    id: nextEnvId++,
    key,
    value,
  }));
}

function buildEnvVars(vars: EnvVar[]): Record<string, string> {
  const result: Record<string, string> = {};
  for (const v of vars) {
    if (v.key.trim()) {
      result[v.key.trim()] = v.value;
    }
  }
  return result;
}

function initialType(extension?: ExtensionEntry): ExtensionModalType {
  if (!extension) return "stdio";
  if (extension.type === "stdio" || extension.type === "streamable_http") {
    return extension.type;
  }
  return "unsupported";
}

function initialEnvVars(extension?: ExtensionEntry): EnvVar[] {
  if (extension?.type === "stdio") return parseEnvVars(extension.envs);
  if (extension?.type === "streamable_http")
    return parseEnvVars(extension.envs);
  return [newEmptyEnvVar()];
}

export function useExtensionModalForm(extension?: ExtensionEntry) {
  const [name, setName] = useState(extension?.name ?? "");
  const [type, setType] = useState<ExtensionModalType>(() =>
    initialType(extension),
  );
  const [description, setDescription] = useState(extension?.description ?? "");
  const [cmd, setCmd] = useState(
    extension?.type === "stdio" ? extension.cmd : "",
  );
  const [args, setArgs] = useState(
    extension?.type === "stdio" ? extension.args.join("\n") : "",
  );
  const [uri, setUri] = useState(
    extension?.type === "streamable_http" ? extension.uri : "",
  );
  const [timeout, setTimeout] = useState(
    String(
      extension?.type === "stdio" || extension?.type === "streamable_http"
        ? (extension.timeout ?? 300)
        : 300,
    ),
  );
  const [envVars, setEnvVars] = useState<EnvVar[]>(() =>
    initialEnvVars(extension),
  );

  const canSubmit =
    type !== "unsupported" &&
    name.trim().length > 0 &&
    (type === "stdio" ? cmd.trim().length > 0 : uri.trim().length > 0);

  const updateEnvVar = (
    index: number,
    field: "key" | "value",
    value: string,
  ) => {
    setEnvVars((prev) => {
      const next = [...prev];
      next[index] = { ...next[index], [field]: value };
      return next;
    });
  };

  const addEnvVar = () => {
    setEnvVars((prev) => [...prev, newEmptyEnvVar()]);
  };

  const removeEnvVar = (id: number) => {
    setEnvVars((prev) => {
      if (prev.length <= 1) return [newEmptyEnvVar()];
      return prev.filter((v) => v.id !== id);
    });
  };

  const buildSubmitPayload = (): {
    name: string;
    config: ExtensionConfig;
  } | null => {
    if (!canSubmit) return null;

    const trimmedName = name.trim();
    const envs = buildEnvVars(envVars);
    const timeoutNum = Number.parseInt(timeout, 10) || 300;

    if (type === "stdio") {
      return {
        name: trimmedName,
        config: {
          ...(extension?.type === "stdio" ? extension : {}),
          type: "stdio",
          name: trimmedName,
          description,
          cmd: cmd.trim(),
          args: args
            .split("\n")
            .map((arg) => arg.trim())
            .filter(Boolean),
          envs,
          timeout: timeoutNum,
        },
      };
    }

    return {
      name: trimmedName,
      config: {
        ...(extension?.type === "streamable_http" ? extension : {}),
        type: "streamable_http",
        name: trimmedName,
        description,
        uri: uri.trim(),
        envs,
        timeout: timeoutNum,
      },
    };
  };

  return {
    name,
    setName,
    type,
    setType,
    description,
    setDescription,
    cmd,
    setCmd,
    args,
    setArgs,
    uri,
    setUri,
    timeout,
    setTimeout,
    envVars,
    canSubmit,
    updateEnvVar,
    addEnvVar,
    removeEnvVar,
    buildSubmitPayload,
  };
}
