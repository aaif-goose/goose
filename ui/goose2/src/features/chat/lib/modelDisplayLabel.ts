import type { ModelOption } from "../types";

interface ModelDisplayLabelOptions {
  currentModelId?: string | null;
  currentModelName?: string | null;
  currentModelProviderId?: string | null;
  availableModels?: ModelOption[];
}

interface PickerTriggerLabelOptions extends ModelDisplayLabelOptions {
  selectedAgentLabel?: string | null;
}

function normalizeLabel(label?: string | null) {
  const trimmed = label?.trim();
  return trimmed ? trimmed : null;
}

function getModelDisplayName(model: ModelOption) {
  return normalizeLabel(model.displayName) ?? normalizeLabel(model.name);
}

function findSelectedInventoryModel({
  currentModelId,
  currentModelProviderId,
  availableModels = [],
}: Pick<
  ModelDisplayLabelOptions,
  "currentModelId" | "currentModelProviderId" | "availableModels"
>) {
  const selectedModelId = normalizeLabel(currentModelId);
  if (!selectedModelId) {
    return null;
  }

  const matches = availableModels.filter(
    (model) => model.id === selectedModelId,
  );
  if (matches.length === 0) {
    return null;
  }

  if (currentModelProviderId) {
    return (
      matches.find((model) => model.providerId === currentModelProviderId) ??
      matches.find((model) => !model.providerId) ??
      null
    );
  }

  return matches[0] ?? null;
}

export function resolveDisplayModelLabel({
  currentModelId,
  currentModelName,
  currentModelProviderId,
  availableModels = [],
}: ModelDisplayLabelOptions) {
  const inventoryModel = findSelectedInventoryModel({
    currentModelId,
    currentModelProviderId,
    availableModels,
  });
  const inventoryLabel = inventoryModel
    ? getModelDisplayName(inventoryModel)
    : null;
  if (inventoryLabel) {
    return inventoryLabel;
  }

  const selectedModelId = normalizeLabel(currentModelId);
  const modelName = normalizeLabel(currentModelName);
  if (modelName && modelName !== selectedModelId) {
    return modelName;
  }

  return null;
}

export function resolvePickerTriggerLabel({
  selectedAgentLabel,
  ...modelOptions
}: PickerTriggerLabelOptions) {
  return (
    resolveDisplayModelLabel(modelOptions) ?? normalizeLabel(selectedAgentLabel)
  );
}
