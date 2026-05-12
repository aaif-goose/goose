import { describe, expect, it } from "vitest";
import type { ExtensionEntry } from "../../types";
import {
  buildExtensionEnvConfig,
  buildExtensionSubmitPayload,
  canSubmitExtensionConfig,
  parseExtensionEnvRows,
} from "../extensionFormConfig";

describe("extensionFormConfig", () => {
  it("combines legacy envs and secret env keys without duplicates", () => {
    expect(
      parseExtensionEnvRows(
        { LEGACY_TOKEN: "plain", SHARED_TOKEN: "plain-shared" },
        ["GITHUB_TOKEN", "SHARED_TOKEN"],
      ),
    ).toEqual([
      { key: "LEGACY_TOKEN", value: "plain" },
      { key: "SHARED_TOKEN", value: "plain-shared" },
      { key: "GITHUB_TOKEN", value: "" },
    ]);
  });

  it("builds envs for populated values and env_keys for blank values", () => {
    expect(
      buildExtensionEnvConfig([
        { key: " GITHUB_TOKEN ", value: "secret" },
        { key: " API_KEY ", value: "" },
        { key: " ", value: "ignored" },
      ]),
    ).toEqual({
      envs: { GITHUB_TOKEN: "secret" },
      env_keys: ["API_KEY"],
    });
  });

  it("validates streamable HTTP URLs by scheme", () => {
    expect(
      canSubmitExtensionConfig({
        type: "streamable_http",
        name: "Context7",
        cmd: "",
        uri: "https://mcp.context7.com/mcp",
      }),
    ).toBe(true);
    expect(
      canSubmitExtensionConfig({
        type: "streamable_http",
        name: "Local file",
        cmd: "",
        uri: "file:///tmp/mcp",
      }),
    ).toBe(false);
  });

  it("builds clean submit payloads while preserving streamable-only fields", () => {
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

    const payload = buildExtensionSubmitPayload({
      type: "streamable_http",
      name: " context7 ",
      description: "Docs",
      cmd: "",
      args: "",
      uri: " https://new.example/mcp ",
      timeout: "90",
      envVars: [{ key: "API_KEY", value: "" }],
      extension,
    });

    expect(payload).toEqual({
      name: "context7",
      config: {
        type: "streamable_http",
        name: "context7",
        description: "Docs",
        uri: "https://new.example/mcp",
        env_keys: ["API_KEY"],
        headers: { Authorization: "Bearer token" },
        socket: "/tmp/mcp.sock",
        timeout: 90,
      },
    });
    expect(payload?.config).not.toHaveProperty("config_key");
    expect(payload?.config).not.toHaveProperty("enabled");
  });
});
