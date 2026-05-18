import type {
  ProviderSetupCatalogEntryDto,
  ProviderSetupCategoryDto,
  ProviderSetupFieldDto,
  ProviderSetupMethodDto,
  ProviderSetupGroupDto,
} from "@aaif/goose-sdk";

export type ProviderCategory = ProviderSetupCategoryDto;
export type ProviderSetupMethod = ProviderSetupMethodDto;
export type ProviderGroup = ProviderSetupGroupDto;
export type ProviderField = ProviderSetupFieldDto;

export type { ProviderConfigFieldValueDto as ProviderFieldValue } from "@aaif/goose-sdk";

export type ProviderCatalogEntry = Omit<
  ProviderSetupCatalogEntryDto,
  | "providerId"
  | "name"
  | "nativeConnectQuery"
  | "binaryName"
  | "docUrl"
  | "showOnlyWhenInstalled"
  | "supportsInstall"
  | "supportsAuth"
  | "supportsAuthStatus"
> & {
  id: string;
  displayName: string;
  nativeConnectQuery?: NonNullable<
    ProviderSetupCatalogEntryDto["nativeConnectQuery"]
  >;
  binaryName?: NonNullable<ProviderSetupCatalogEntryDto["binaryName"]>;
  docsUrl?: NonNullable<ProviderSetupCatalogEntryDto["docUrl"]>;
  showOnlyWhenInstalled?: ProviderSetupCatalogEntryDto["showOnlyWhenInstalled"];
  supportsInstall?: ProviderSetupCatalogEntryDto["supportsInstall"];
  supportsAuth?: ProviderSetupCatalogEntryDto["supportsAuth"];
  supportsAuthStatus?: ProviderSetupCatalogEntryDto["supportsAuthStatus"];
};

export type ProviderSetupStatus =
  | "built_in"
  | "connected"
  | "not_installed"
  | "not_configured"
  | "installing"
  | "authenticating"
  | "error";

export interface ProviderDisplayInfo extends ProviderCatalogEntry {
  status: ProviderSetupStatus;
}
