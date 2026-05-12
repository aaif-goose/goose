export function parseCustomProviderModels(input: string | string[]): string[] {
  const rawModels = Array.isArray(input)
    ? input
    : input.split(/[\n,]/).map((value) => value.trim());

  const seen = new Set<string>();
  const models: string[] = [];

  for (const rawModel of rawModels) {
    const model = rawModel.trim();
    if (!model || seen.has(model)) {
      continue;
    }
    seen.add(model);
    models.push(model);
  }

  return models;
}

export function formatCustomProviderModels(models: string[]): string {
  return parseCustomProviderModels(models).join(", ");
}
