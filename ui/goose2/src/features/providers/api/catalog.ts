import type {
  ProviderSetupCatalogEntryDto,
  ProviderSetupFieldDto,
} from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import type {
  ProviderCatalogEntry,
  ProviderField,
} from "@/shared/types/providers";
import { perfLog } from "@/shared/lib/perfLog";

function toOptionalString(
  value: string | null | undefined,
): string | undefined {
  return value || undefined;
}

function toAliases(value: string[] | null | undefined): string[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }

  const aliases = value.filter(Boolean);
  return aliases.length > 0 ? aliases : undefined;
}

function toProviderField(field: ProviderSetupFieldDto): ProviderField {
  return {
    key: field.key,
    label: field.label,
    secret: field.secret,
    required: field.required,
    ...(field.placeholder ? { placeholder: field.placeholder } : {}),
    ...(field.defaultValue ? { defaultValue: field.defaultValue } : {}),
  };
}

export function mapProviderCatalogEntryDto(
  dto: ProviderSetupCatalogEntryDto,
): ProviderCatalogEntry {
  const entry: ProviderCatalogEntry = {
    id: dto.providerId,
    displayName: dto.name || dto.providerId,
    category: dto.category,
    description: dto.description,
    setupMethod: dto.setupMethod,
    tier: dto.tier,
  };

  const nativeConnectQuery = toOptionalString(dto.nativeConnectQuery);
  if (nativeConnectQuery) entry.nativeConnectQuery = nativeConnectQuery;

  if (dto.fields) entry.fields = dto.fields.map(toProviderField);

  const binaryName = toOptionalString(dto.binaryName);
  if (binaryName) entry.binaryName = binaryName;

  const docsUrl = toOptionalString(dto.docUrl);
  if (docsUrl) entry.docsUrl = docsUrl;

  if (typeof dto.showOnlyWhenInstalled === "boolean") {
    entry.showOnlyWhenInstalled = dto.showOnlyWhenInstalled;
  }

  const aliases = toAliases(dto.aliases);
  if (aliases) entry.aliases = aliases;

  if (typeof dto.supportsInstall === "boolean") {
    entry.supportsInstall = dto.supportsInstall;
  }
  if (typeof dto.supportsAuth === "boolean") {
    entry.supportsAuth = dto.supportsAuth;
  }
  if (typeof dto.supportsAuthStatus === "boolean") {
    entry.supportsAuthStatus = dto.supportsAuthStatus;
  }

  return entry;
}

export async function listProviderSetupCatalog(): Promise<
  ProviderCatalogEntry[]
> {
  const client = await getClient();
  const t0 = performance.now();
  const response = await client.goose.GooseProvidersSetupCatalogList({});
  const providers = response.providers.map(mapProviderCatalogEntryDto);

  perfLog(
    `[perf:catalog] listProviderSetupCatalog done in ${(performance.now() - t0).toFixed(1)}ms (n=${providers.length})`,
  );
  return providers;
}
