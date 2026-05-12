import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ExtensionEntry } from "../../types";
import { useExtensionModalForm } from "../useExtensionModalForm";

describe("useExtensionModalForm", () => {
  it("builds trimmed stdio configs with args, env vars, and timeout", () => {
    const { result } = renderHook(() => useExtensionModalForm());

    act(() => {
      result.current.setName(" GitHub MCP ");
      result.current.setDescription("Issue tools");
      result.current.setCmd(" npx ");
      result.current.setArgs(" -y \n @modelcontextprotocol/server-github \n\n");
      result.current.setTimeout("45");
      result.current.updateEnvVar(0, "key", " GITHUB_TOKEN ");
      result.current.updateEnvVar(0, "value", "secret");
    });
    act(() => {
      result.current.addEnvVar();
    });
    act(() => {
      result.current.updateEnvVar(1, "key", " ");
      result.current.updateEnvVar(1, "value", "ignored");
    });

    expect(result.current.buildSubmitPayload()).toEqual({
      name: "GitHub MCP",
      config: {
        type: "stdio",
        name: "GitHub MCP",
        description: "Issue tools",
        cmd: "npx",
        args: ["-y", "@modelcontextprotocol/server-github"],
        envs: { GITHUB_TOKEN: "secret" },
        timeout: 45,
      },
    });
  });

  it("builds streamable HTTP configs and falls back to the default timeout", () => {
    const { result } = renderHook(() => useExtensionModalForm());

    act(() => {
      result.current.setType("streamable_http");
      result.current.setName(" Context7 ");
      result.current.setDescription("Docs");
      result.current.setUri(" https://mcp.context7.com/mcp ");
      result.current.setTimeout("");
    });

    expect(result.current.buildSubmitPayload()).toEqual({
      name: "Context7",
      config: {
        type: "streamable_http",
        name: "Context7",
        description: "Docs",
        uri: "https://mcp.context7.com/mcp",
        timeout: 300,
      },
    });
  });

  it("preserves editable config fields without submitting entry fields", () => {
    const extension: ExtensionEntry = {
      type: "streamable_http",
      name: "context7",
      description: "Docs",
      uri: "https://old.example/mcp",
      env_keys: ["API_KEY"],
      headers: { Authorization: "Bearer token" },
      socket: "/tmp/mcp.sock",
      config_key: "context7",
      enabled: true,
      timeout: 60,
    };
    const { result } = renderHook(() => useExtensionModalForm(extension));

    act(() => {
      result.current.setUri("https://new.example/mcp");
    });

    expect(result.current.buildSubmitPayload()?.config).toMatchObject({
      type: "streamable_http",
      name: "context7",
      uri: "https://new.example/mcp",
      env_keys: ["API_KEY"],
      headers: { Authorization: "Bearer token" },
      socket: "/tmp/mcp.sock",
      timeout: 60,
    });
    expect(result.current.buildSubmitPayload()?.config).not.toHaveProperty(
      "config_key",
    );
    expect(result.current.buildSubmitPayload()?.config).not.toHaveProperty(
      "enabled",
    );
  });

  it("keeps secret env keys visible and preserves them when unchanged", () => {
    const extension: ExtensionEntry = {
      type: "stdio",
      name: "github",
      description: "Issue tools",
      cmd: "npx",
      args: [],
      envs: { LEGACY_TOKEN: "plain" },
      env_keys: ["GITHUB_TOKEN"],
      config_key: "github",
      enabled: false,
    };
    const { result } = renderHook(() => useExtensionModalForm(extension));

    expect(result.current.envVars).toMatchObject([
      { key: "LEGACY_TOKEN", value: "plain" },
      { key: "GITHUB_TOKEN", value: "" },
    ]);
    expect(result.current.buildSubmitPayload()?.config).toMatchObject({
      envs: { LEGACY_TOKEN: "plain" },
      env_keys: ["GITHUB_TOKEN"],
    });
  });

  it("rejects non-HTTP streamable HTTP URIs", () => {
    const { result } = renderHook(() => useExtensionModalForm());

    act(() => {
      result.current.setType("streamable_http");
      result.current.setName("Local file");
      result.current.setUri("file:///tmp/mcp");
    });

    expect(result.current.canSubmit).toBe(false);
    expect(result.current.buildSubmitPayload()).toBeNull();
  });

  it("does not coerce unsupported SSE extensions into HTTP configs", () => {
    const extension: ExtensionEntry = {
      type: "sse",
      name: "legacy-sse",
      description: "Legacy SSE endpoint",
      uri: "https://old.example/sse",
      config_key: "legacy-sse",
      enabled: true,
    };
    const { result } = renderHook(() => useExtensionModalForm(extension));

    expect(result.current.type).toBe("unsupported");
    expect(result.current.canSubmit).toBe(false);
    expect(result.current.buildSubmitPayload()).toBeNull();
  });

  it("returns null when required fields are missing", () => {
    const { result } = renderHook(() => useExtensionModalForm());

    act(() => {
      result.current.setName("No command");
    });

    expect(result.current.canSubmit).toBe(false);
    expect(result.current.buildSubmitPayload()).toBeNull();
  });
});
