import type {
  ProviderCatalogEntryDto,
  ProviderSetupFieldDto,
} from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import type {
  ProviderCatalogEntry,
  ProviderCategory,
  ProviderField,
  ProviderSetupMethod,
  ProviderTier,
} from "@/shared/types/providers";
import { perfLog } from "@/shared/lib/perfLog";

const PROVIDER_CATEGORIES = new Set<ProviderCategory>(["agent", "model"]);
const PROVIDER_SETUP_METHODS = new Set<ProviderSetupMethod>([
  "none",
  "single_api_key",
  "config_fields",
  "host_with_oauth_fallback",
  "oauth_browser",
  "oauth_device_code",
  "cloud_credentials",
  "local",
  "cli_auth",
]);
const PROVIDER_TIERS = new Set<ProviderTier>([
  "promoted",
  "standard",
  "advanced",
]);

function toOptionalString(
  value: string | null | undefined,
): string | undefined {
  return value || undefined;
}

function toCategory(value: string | null | undefined): ProviderCategory | null {
  return value && PROVIDER_CATEGORIES.has(value as ProviderCategory)
    ? (value as ProviderCategory)
    : null;
}

function toSetupMethod(value: string | null | undefined): ProviderSetupMethod {
  return value && PROVIDER_SETUP_METHODS.has(value as ProviderSetupMethod)
    ? (value as ProviderSetupMethod)
    : "none";
}

function toTier(value: string | null | undefined): ProviderTier {
  return value && PROVIDER_TIERS.has(value as ProviderTier)
    ? (value as ProviderTier)
    : "standard";
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
  dto: ProviderCatalogEntryDto,
): ProviderCatalogEntry | null {
  const category = toCategory(dto.category);
  if (!category) {
    return null;
  }

  const entry: ProviderCatalogEntry = {
    id: dto.providerId,
    displayName: dto.name || dto.providerId,
    category,
    description: dto.description ?? "",
    setupMethod: toSetupMethod(dto.setupMethod),
    tier: toTier(dto.tier),
  };

  const nativeConnectQuery = toOptionalString(dto.nativeConnectQuery);
  if (nativeConnectQuery) entry.nativeConnectQuery = nativeConnectQuery;

  const envVar = toOptionalString(dto.envVar);
  if (envVar) entry.envVar = envVar;

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
  const response = await client.goose.GooseProvidersCatalogList({
    kind: "setup",
  });
  const providers = response.providers
    .map(mapProviderCatalogEntryDto)
    .filter((entry): entry is ProviderCatalogEntry => entry !== null);

  perfLog(
    `[perf:catalog] listProviderSetupCatalog done in ${(performance.now() - t0).toFixed(1)}ms (n=${providers.length})`,
  );
  return providers;
}
