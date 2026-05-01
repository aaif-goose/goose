type ReplayMetadataSource = {
  _meta?: Record<string, unknown> | null;
  messageId?: string | null;
};

export function getReplayMessageId(
  source: ReplayMetadataSource,
): string | null {
  if (source.messageId) {
    return source.messageId;
  }

  const metaMessageId = getGooseReplayMeta(source)?.messageId;
  if (typeof metaMessageId === "string" && metaMessageId.length > 0) {
    return metaMessageId;
  }

  return null;
}

export function getReplayCreated(
  source: ReplayMetadataSource,
): number | undefined {
  const goose = getGooseReplayMeta(source);
  return coerceReplayTimestamp(goose?.created ?? goose?.createdAt);
}

function getGooseReplayMeta(
  source: ReplayMetadataSource,
): Record<string, unknown> | null {
  if (!isRecord(source._meta)) {
    return null;
  }

  const goose = source._meta.goose;
  return isRecord(goose) ? goose : null;
}

function coerceReplayTimestamp(value: unknown): number | undefined {
  if (typeof value === "number") {
    return normalizeEpochMilliseconds(value);
  }

  return undefined;
}

function normalizeEpochMilliseconds(value: number): number | undefined {
  if (!Number.isFinite(value) || value < 0) {
    return undefined;
  }

  return value < 1_000_000_000_000 ? value * 1000 : value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
