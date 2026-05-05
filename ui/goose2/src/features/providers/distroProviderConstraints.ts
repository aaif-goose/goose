import type { ProviderCatalogEntry } from "@/shared/types/providers";
import type { DistroBundleInfo } from "@/shared/types/distro";

export function parseProviderAllowlist(
  distro: DistroBundleInfo | null | undefined,
): Set<string> | null {
  if (!distro?.present) {
    return null;
  }

  const raw = distro.providerAllowlist?.trim();
  if (!raw) {
    return null;
  }

  const providerIds = raw
    .split(",")
    .map((providerId) => providerId.trim())
    .filter(Boolean);

  return providerIds.length > 0 ? new Set(providerIds) : null;
}

export function filterModelProvidersForDistro(
  providers: ProviderCatalogEntry[],
  distro: DistroBundleInfo | null | undefined,
): ProviderCatalogEntry[] {
  const allowlist = parseProviderAllowlist(distro);
  if (!allowlist) {
    return providers;
  }

  return providers.filter((provider) => allowlist.has(provider.id));
}

export function isProviderAllowedByAllowlist(
  providerId: string,
  allowlist: Set<string> | null,
): boolean {
  return !allowlist || allowlist.has(providerId);
}

export function hasAllowedModelProvider(
  providers: Pick<ProviderCatalogEntry, "id">[],
  allowlist: Set<string> | null,
): boolean {
  return providers.some((provider) =>
    isProviderAllowedByAllowlist(provider.id, allowlist),
  );
}
