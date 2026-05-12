export interface DistroBundleInfo {
  present: boolean;
  appVersion?: string;
  featureToggles?: Record<string, boolean>;
  extensionAllowlist?: string;
  providerAllowlist?: string;
}
