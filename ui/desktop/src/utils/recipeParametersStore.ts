// Bridges URL-supplied recipe parameters from the warm-launch IPC path into
// ParameterInputModal. The cold-launch path seeds the same data into
// window.appConfig.recipeParameters, which is fixed at preload and cannot be
// mutated after window load.

const store = new Map<string, Record<string, string>>();

export function setRecipeParametersForSession(
  sessionId: string,
  parameters: Record<string, string> | undefined
): void {
  if (parameters && Object.keys(parameters).length > 0) {
    store.set(sessionId, parameters);
  }
}

export function takeRecipeParametersForSession(
  sessionId: string
): Record<string, string> | undefined {
  const value = store.get(sessionId);
  if (value) {
    store.delete(sessionId);
  }
  return value;
}
