import type {
  DefaultsReadResponse,
  DefaultsSaveRequest,
  OnboardingImportApplyRequest,
  OnboardingImportApplyResponse,
  OnboardingImportCandidate,
} from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";

export async function readDefaults(): Promise<DefaultsReadResponse> {
  const client = await getClient();
  return client.goose.GooseDefaultsRead({});
}

export async function saveDefaults(
  params: DefaultsSaveRequest,
): Promise<DefaultsReadResponse> {
  const client = await getClient();
  return client.goose.GooseDefaultsSave(params);
}

export async function scanOnboardingImports(): Promise<
  OnboardingImportCandidate[]
> {
  const client = await getClient();
  const response = await client.goose.GooseOnboardingImportScan({
    sources: [],
  });
  return response.candidates;
}

export async function applyOnboardingImports(
  params: OnboardingImportApplyRequest,
): Promise<OnboardingImportApplyResponse> {
  const client = await getClient();
  return client.goose.GooseOnboardingImportApply(params);
}
