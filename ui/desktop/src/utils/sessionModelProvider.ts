export interface ResolveModelProviderInput {
  sessionId: string | null;
  sessionModel?: string | null;
  sessionProvider?: string | null;
  configModel?: string | null;
  configProvider?: string | null;
  override?: { model: string; provider: string } | null;
}

export interface ResolveModelProviderOutput {
  model: string | null;
  provider: string | null;
  isSessionScoped: boolean;
}

/**
 * Resolve effective model/provider with session-first semantics.
 *
 * Rules:
 * - Explicit local override wins.
 * - If a session is active, only session-scoped values are used.
 *   (No fallback to global config while session context is loading.)
 * - If no session is active, fall back to global config defaults.
 */
export function resolveModelProvider({
  sessionId,
  sessionModel,
  sessionProvider,
  configModel,
  configProvider,
  override,
}: ResolveModelProviderInput): ResolveModelProviderOutput {
  if (override) {
    return {
      model: override.model,
      provider: override.provider,
      isSessionScoped: !!sessionId,
    };
  }

  const isSessionScoped = !!sessionId;

  if (isSessionScoped) {
    return {
      model: sessionModel ?? null,
      provider: sessionProvider ?? null,
      isSessionScoped: true,
    };
  }

  return {
    model: configModel ?? null,
    provider: configProvider ?? null,
    isSessionScoped: false,
  };
}
