import type { ProviderSetupCatalogEntryDto } from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import type { ProviderCatalogEntry } from "@/shared/types/providers";
import { perfLog } from "@/shared/lib/perfLog";

export function mapProviderSetupCatalogEntryDto(
  dto: ProviderSetupCatalogEntryDto,
): ProviderCatalogEntry {
  return {
    id: dto.providerId,
    displayName: dto.name,
    category: dto.category,
    description: dto.description,
    setupMethod: dto.setupMethod,
    ...(dto.nativeConnectQuery
      ? { nativeConnectQuery: dto.nativeConnectQuery }
      : {}),
    ...(dto.fields?.length ? { fields: dto.fields } : {}),
    ...(dto.binaryName ? { binaryName: dto.binaryName } : {}),
    ...(dto.docUrl ? { docsUrl: dto.docUrl } : {}),
    group: dto.group,
    showOnlyWhenInstalled: dto.showOnlyWhenInstalled,
    ...(dto.aliases?.length ? { aliases: dto.aliases } : {}),
    supportsInstall: dto.supportsInstall,
    supportsAuth: dto.supportsAuth,
    supportsAuthStatus: dto.supportsAuthStatus,
  };
}

export async function listProviderSetupCatalog(): Promise<
  ProviderCatalogEntry[]
> {
  const client = await getClient();
  const t0 = performance.now();
  const response = await client.goose.GooseProvidersSetupCatalogList({});
  const providers = response.providers.map(mapProviderSetupCatalogEntryDto);

  perfLog(
    `[perf:catalog] listProviderSetupCatalog done in ${(performance.now() - t0).toFixed(1)}ms (n=${providers.length})`,
  );
  return providers;
}
