import { useState } from "react";
import {
  buildExtensionSubmitPayload,
  canSubmitExtensionConfig,
  parseExtensionEnvRows,
  type ExtensionEnvRow,
  type ExtensionModalType,
} from "../lib/extensionFormConfig";
import type { ExtensionConfig, ExtensionEntry } from "../types";

export type { ExtensionModalType };

export interface EnvVar extends ExtensionEnvRow {
  id: number;
}

let nextEnvId = 0;

function newEmptyEnvVar(): EnvVar {
  return { id: nextEnvId++, key: "", value: "" };
}

function withEnvIds(rows: ExtensionEnvRow[]): EnvVar[] {
  return rows.length > 0
    ? rows.map((row) => ({ id: nextEnvId++, ...row }))
    : [newEmptyEnvVar()];
}

function initialType(extension?: ExtensionEntry): ExtensionModalType {
  if (!extension) return "stdio";
  if (extension.type === "stdio" || extension.type === "streamable_http") {
    return extension.type;
  }
  return "unsupported";
}

function initialEnvVars(extension?: ExtensionEntry): EnvVar[] {
  if (extension?.type === "stdio")
    return withEnvIds(
      parseExtensionEnvRows(extension.envs, extension.env_keys),
    );
  if (extension?.type === "streamable_http")
    return withEnvIds(
      parseExtensionEnvRows(extension.envs, extension.env_keys),
    );
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

  const canSubmit = canSubmitExtensionConfig({ type, name, cmd, uri });

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
    return buildExtensionSubmitPayload({
      type,
      name,
      description,
      cmd,
      args,
      uri,
      timeout,
      envVars,
      extension,
    });
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
