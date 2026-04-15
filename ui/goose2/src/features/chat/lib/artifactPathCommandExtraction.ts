type ToolNamePathCandidate = {
  rawPath: string;
  source: "arg_key" | "result_regex";
  confidence: "high" | "low";
  fromResultText: boolean;
};

const COMMAND_ARG_KEYS = ["cmd", "command"] as const;

function stripEnclosingQuotes(token: string): string {
  const trimmed = token.trim();
  if (!trimmed) return "";
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'")) ||
    (trimmed.startsWith("`") && trimmed.endsWith("`"))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function splitCommandTokens(command: string): string[] {
  // Preserve quoted segments so paths with spaces stay intact.
  const tokens = command.match(/"[^"]*"|'[^']*'|`[^`]*`|\S+/g);
  return tokens ?? [];
}

function extractCommandPathCandidates(
  command: string,
  isLikelyLocalPath: (candidate: string) => boolean,
  stripTokenPunctuation: (token: string) => string,
): string[] {
  const tokens = splitCommandTokens(command);
  if (tokens.length === 0) return [];

  const candidates: string[] = [];
  const pushIfLocalPath = (rawToken?: string) => {
    if (!rawToken) return;
    const normalized = stripTokenPunctuation(stripEnclosingQuotes(rawToken));
    if (!isLikelyLocalPath(normalized)) return;
    candidates.push(normalized);
  };

  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    const normalizedToken = token.toLowerCase();

    if (/^(?:\d?>|>>|\d?>>)$/.test(token)) {
      pushIfLocalPath(tokens[index + 1]);
      continue;
    }

    if (
      normalizedToken === "-o" ||
      normalizedToken === "--output" ||
      normalizedToken === "--out" ||
      normalizedToken === "--save" ||
      normalizedToken === "--file" ||
      normalizedToken === "--path" ||
      normalizedToken === "--output-file"
    ) {
      pushIfLocalPath(tokens[index + 1]);
      continue;
    }

    const equalsMatch = token.match(
      /^--(?:output|out|save|file|path|output-file)=(.+)$/i,
    );
    if (equalsMatch) {
      pushIfLocalPath(equalsMatch[1]);
    }
  }

  return candidates;
}

export function extractToolNamePathCandidates(
  toolName: string,
  isLikelyLocalPath: (candidate: string) => boolean,
  stripTokenPunctuation: (token: string) => string,
): ToolNamePathCandidate[] {
  const matches: ToolNamePathCandidate[] = [];
  const trimmed = toolName.trim();
  if (!trimmed) return matches;

  const match = trimmed.match(/^(write|create|save)\s+(.+)$/i);
  if (match) {
    const remainder = match[2].trim();
    if (remainder) {
      const quoted = remainder.match(/^["'`](.+?)["'`]$/);
      const token = quoted ? quoted[1] : remainder.split(/\s+/)[0];
      const cleaned = stripTokenPunctuation(token);
      if (isLikelyLocalPath(cleaned)) {
        matches.push({
          rawPath: cleaned,
          source: "arg_key",
          confidence: "high",
          fromResultText: false,
        });
      }
    }
  }

  for (const rawPath of extractCommandPathCandidates(
    trimmed,
    isLikelyLocalPath,
    stripTokenPunctuation,
  )) {
    matches.push({
      rawPath,
      source: "result_regex",
      confidence: "low",
      fromResultText: false,
    });
  }

  return matches;
}

export function collectCommandArgPathCandidates(
  args: Record<string, unknown>,
  isLikelyLocalPath: (candidate: string) => boolean,
  stripTokenPunctuation: (token: string) => string,
): string[] {
  const values: string[] = [];
  for (const key of COMMAND_ARG_KEYS) {
    const value = args[key];
    if (typeof value !== "string" || !value.trim()) continue;
    values.push(
      ...extractCommandPathCandidates(
        value,
        isLikelyLocalPath,
        stripTokenPunctuation,
      ),
    );
  }
  return values;
}
