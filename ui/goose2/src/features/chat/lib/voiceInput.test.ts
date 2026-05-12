import { describe, expect, it } from "vitest";
import {
  appendTranscribedText,
  getDefaultDictationProvider,
  getAutoSubmitMatch,
  parseAutoSubmitPhrases,
  replaceTrailingTranscribedText,
} from "./voiceInput";

describe("voiceInput helpers", () => {
  it("parses comma-separated auto-submit phrases", () => {
    expect(parseAutoSubmitPhrases(" submit, Ship It ,submit ,, ")).toEqual([
      "submit",
      "ship it",
    ]);
  });

  it("appends dictated text without smashing words together", () => {
    expect(appendTranscribedText("hello", "world")).toBe("hello world");
    expect(appendTranscribedText("hello ", "world")).toBe("hello world");
    expect(appendTranscribedText("hello", ", world")).toBe("hello, world");
  });

  it("replaces only the trailing dictated segment", () => {
    expect(
      replaceTrailingTranscribedText(
        "draft dictated text",
        "dictated text",
        "dictated text submit",
      ),
    ).toBe("draft dictated text submit");
  });

  it("matches auto-submit phrases only at the end of dictated text", () => {
    expect(getAutoSubmitMatch("please submit now", ["submit"])).toBeNull();
    expect(getAutoSubmitMatch("please SUBMIT.", ["submit"])).toEqual({
      matchedPhrase: "submit",
      textWithoutPhrase: "please",
    });
  });

  it("strips the full raw phrase span when internal whitespace is repeated", () => {
    // The phrase "ship it" is matched against the *normalized* text, where
    // "ship   it" collapses to "ship it" (7 chars). But in the raw text the
    // phrase occupies 9 chars — slicing by -phrase.length would leave a
    // dangling "sh" on the end. The fix walks the raw text with a regex so
    // the slice index reflects the actual phrase span in the raw string.
    expect(getAutoSubmitMatch("hello ship   it", ["ship it"])).toEqual({
      matchedPhrase: "ship it",
      textWithoutPhrase: "hello",
    });
  });

  it("picks the first configured dictation provider by priority", () => {
    expect(
      getDefaultDictationProvider({
        openai: {
          configured: false,
          description: "OpenAI",
          usesProviderConfig: true,
          availableModels: [],
        },
        groq: {
          configured: true,
          description: "Groq",
          usesProviderConfig: false,
          availableModels: [],
        },
        local: {
          configured: true,
          description: "Local",
          usesProviderConfig: false,
          availableModels: [],
        },
      }),
    ).toBe("groq");
  });

  it("falls back to the first available provider when none are configured", () => {
    expect(
      getDefaultDictationProvider({
        elevenlabs: {
          configured: false,
          description: "ElevenLabs",
          usesProviderConfig: false,
          availableModels: [],
        },
        local: {
          configured: false,
          description: "Local",
          usesProviderConfig: false,
          availableModels: [],
        },
      }),
    ).toBe("local");
  });
});
