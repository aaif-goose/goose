import { describe, expect, it } from "vitest";
import {
  getChatInputAgentLabel,
  getChatInputPlaceholder,
} from "./chatInputPlaceholder";

const t = (key: string, options?: { agent: string }) =>
  options?.agent ? `${key}:${options.agent}` : key;

describe("getChatInputAgentLabel", () => {
  it("uses the active persona display name when present", () => {
    expect(getChatInputAgentLabel("Reviewer", "Goose")).toBe("Reviewer");
  });

  it("falls back to the provider display name", () => {
    expect(getChatInputAgentLabel(undefined, "Goose")).toBe("Goose");
  });

  it("preserves explicit persona names with the default suffix", () => {
    expect(getChatInputAgentLabel("Ops (Default)", "Goose (Default)")).toBe(
      "Ops (Default)",
    );
  });

  it("removes the default suffix from provider fallback labels", () => {
    expect(getChatInputAgentLabel(undefined, "Goose (Default)")).toBe("Goose");
  });
});

describe("getChatInputPlaceholder", () => {
  it("uses the agent label in the default placeholder", () => {
    expect(getChatInputPlaceholder(t, "Goose", false, false)).toBe(
      "input.placeholder:Goose",
    );
  });

  it("uses voice status placeholders while recording or transcribing", () => {
    expect(getChatInputPlaceholder(t, "Goose", true, false)).toBe(
      "toolbar.voiceInputRecording",
    );
    expect(getChatInputPlaceholder(t, "Goose", false, true)).toBe(
      "toolbar.voiceInputTranscribing",
    );
  });
});
