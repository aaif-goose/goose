import type {
  DictationProvider,
  DictationProviderStatus,
} from "@/shared/types/dictation";

// goose config keys — stored in the user's goose config.yaml via the
// _goose/config/{read,upsert,remove} ACP methods, not localStorage.
export const VOICE_AUTO_SUBMIT_PHRASES_CONFIG_KEY = "VOICE_AUTO_SUBMIT_PHRASES";
export const VOICE_DICTATION_PROVIDER_CONFIG_KEY = "VOICE_DICTATION_PROVIDER";
export const VOICE_DICTATION_PREFERRED_MIC_CONFIG_KEY =
  "VOICE_DICTATION_PREFERRED_MIC";
export const VOICE_DICTATION_CONFIG_EVENT = "goose:voice-dictation-config";
export const DISABLED_DICTATION_PROVIDER_CONFIG_VALUE = "__disabled__";

export const DEFAULT_AUTO_SUBMIT_PHRASES_RAW = "submit";

const TRAILING_PUNCTUATION_REGEX = /[\s"'`.,!?;:)\]}]+$/u;

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function normalizePhrase(value: string): string {
  return value
    .toLowerCase()
    .replace(/\s+/g, " ")
    .trim()
    .replace(TRAILING_PUNCTUATION_REGEX, "")
    .trim();
}

export function parseAutoSubmitPhrases(rawValue: string | null | undefined) {
  if (!rawValue) {
    return [];
  }

  return Array.from(
    new Set(
      rawValue
        .split(",")
        .map((value) => normalizePhrase(value))
        .filter(Boolean),
    ),
  );
}

export function normalizeDictationProvider(
  value: string | null | undefined,
): DictationProvider | null {
  if (
    value === "openai" ||
    value === "groq" ||
    value === "elevenlabs" ||
    value === "local"
  ) {
    return value;
  }

  return null;
}

export function getDefaultDictationProvider(
  providerStatuses: Partial<Record<DictationProvider, DictationProviderStatus>>,
): DictationProvider | null {
  const configuredProviderPriority: DictationProvider[] = [
    "openai",
    "groq",
    "elevenlabs",
    "local",
  ];
  const fallbackProviderPriority: DictationProvider[] = [
    "local",
    "openai",
    "groq",
    "elevenlabs",
  ];

  for (const provider of configuredProviderPriority) {
    if (providerStatuses[provider]?.configured) {
      return provider;
    }
  }

  for (const provider of fallbackProviderPriority) {
    if (providerStatuses[provider]) {
      return provider;
    }
  }

  return null;
}

export function appendTranscribedText(baseText: string, fragment: string) {
  const normalizedFragment = fragment.replace(/\s+/g, " ").trim();
  if (!normalizedFragment) {
    return baseText;
  }

  if (!baseText.trim()) {
    return normalizedFragment;
  }

  if (/[\s([{/-]$/.test(baseText) || /^[,.;!?)]/.test(normalizedFragment)) {
    return `${baseText}${normalizedFragment}`;
  }

  return `${baseText} ${normalizedFragment}`;
}

export function replaceTrailingTranscribedText(
  fullText: string,
  previousTranscribedText: string,
  nextTranscribedText: string,
) {
  if (!previousTranscribedText) {
    return appendTranscribedText(fullText, nextTranscribedText);
  }

  if (fullText.endsWith(previousTranscribedText)) {
    return appendTranscribedText(
      fullText.slice(0, -previousTranscribedText.length),
      nextTranscribedText,
    );
  }

  const trimmedPreviousText = previousTranscribedText.trim();
  if (trimmedPreviousText && fullText.endsWith(trimmedPreviousText)) {
    return appendTranscribedText(
      fullText.slice(0, -trimmedPreviousText.length),
      nextTranscribedText,
    );
  }

  return appendTranscribedText(fullText, nextTranscribedText);
}

export function getAutoSubmitMatch(
  transcribedText: string,
  autoSubmitPhrases: string[],
) {
  const normalizedTranscribedText = normalizePhrase(transcribedText);
  if (!normalizedTranscribedText) {
    return null;
  }

  const sortedPhrases = [...autoSubmitPhrases].sort(
    (left, right) => right.length - left.length,
  );

  for (const phrase of sortedPhrases) {
    if (!normalizedTranscribedText.endsWith(phrase)) {
      continue;
    }

    const phraseStartIndex = normalizedTranscribedText.length - phrase.length;
    if (
      phraseStartIndex > 0 &&
      normalizedTranscribedText[phraseStartIndex - 1] !== " "
    ) {
      continue;
    }

    // Map the phrase back to the *raw* transcribed text. `phrase.length` is
    // the length in normalized form (whitespace collapsed to single spaces,
    // lowercased, trailing punctuation stripped). Applying -phrase.length
    // directly to trimmedText undercounts whenever the raw text has repeated
    // whitespace or mixed case, chopping off legitimate content. Instead,
    // match the phrase at the end of the raw text using a regex that allows
    // flexible whitespace between words, so the slice index reflects the
    // actual start of the phrase in the raw string.
    const trimmedText = transcribedText.replace(TRAILING_PUNCTUATION_REGEX, "");
    const phraseWords = phrase.split(" ").filter(Boolean).map(escapeRegExp);
    const phrasePattern = new RegExp(
      `(^|\\s)(${phraseWords.join("\\s+")})\\s*$`,
      "iu",
    );
    const rawMatch = trimmedText.match(phrasePattern);
    const phraseStartOffset =
      rawMatch && rawMatch.index !== undefined
        ? rawMatch.index + (rawMatch[1]?.length ?? 0)
        : trimmedText.length - phrase.length;
    const textWithoutPhrase = trimmedText.slice(0, phraseStartOffset).trimEnd();

    return {
      matchedPhrase: phrase,
      textWithoutPhrase,
    };
  }

  return null;
}

export function notifyVoiceDictationConfigChanged() {
  try {
    window.dispatchEvent(new Event(VOICE_DICTATION_CONFIG_EVENT));
  } catch {
    // no-op
  }
}
