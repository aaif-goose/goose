export type ProviderCategory = "agent" | "model";

export type ProviderSetupMethod =
  | "none"
  | "single_api_key"
  | "config_fields"
  | "host_with_oauth_fallback"
  | "oauth_browser"
  | "oauth_device_code"
  | "cloud_credentials"
  | "local"
  | "cli_auth";

export type ProviderTier = "promoted" | "standard" | "advanced";

export interface ProviderField {
  key: string;
  label: string;
  secret: boolean;
  required: boolean;
  placeholder?: string;
}

export interface ProviderFieldValue {
  key: string;
  value: string | null;
  isSet: boolean;
  isSecret: boolean;
  required: boolean;
}

export interface ProviderCatalogEntry {
  id: string;
  displayName: string;
  category: ProviderCategory;
  description: string;
  setupMethod: ProviderSetupMethod;
  nativeConnectQuery?: string;
  envVar?: string;
  fields?: ProviderField[];
  binaryName?: string;
  installCommand?: string;
  authCommand?: string;
  authStatusCommand?: string;
  docsUrl?: string;
  tier: ProviderTier;
  showOnlyWhenInstalled?: boolean;
}

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
