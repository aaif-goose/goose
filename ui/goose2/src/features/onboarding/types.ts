import type {
  DefaultsReadResponse,
  ProviderInventoryEntryDto,
  OnboardingImportApplyResponse,
  OnboardingImportCandidate,
  OnboardingImportCounts,
} from "@aaif/goose-sdk";

export const ONBOARDING_STORAGE_KEY = "goose:onboarding:v1";

export type {
  DefaultsReadResponse,
  OnboardingImportApplyResponse,
  OnboardingImportCandidate,
  OnboardingImportCounts,
};

export interface OnboardingCompletion {
  completedAt: string;
  providerId: string;
  modelId?: string;
}

export interface OnboardingReadiness {
  hasCompletedOnboarding: boolean;
  isUsable: boolean;
  providerId: string | null;
  modelId?: string;
  modelName?: string;
  reason: "ready" | "not_completed" | "missing_provider" | "missing_model";
}

export type OnboardingStep = "import" | "provider" | "tour";

export interface SelectedSetup {
  providerId: string;
  modelId?: string;
  modelName?: string;
}

export interface UsableDefaultEntry {
  kind: "model" | "agent";
  entry: ProviderInventoryEntryDto;
}

export interface ProviderFieldSaveInput {
  key: string;
  value: string;
  isSecret: boolean;
}

export type TFunctionLike = (
  key: string,
  options?: Record<string, unknown>,
) => string;
