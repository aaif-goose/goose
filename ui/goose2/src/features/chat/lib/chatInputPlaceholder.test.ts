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

  it("removes the default suffix from placeholder labels", () => {
    expect(getChatInputAgentLabel("Goose (Default)", "Claude Code")).toBe(
      "Goose",
    );
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
