export interface DistroSecurityManifest {
  extensionAllowlist?: string;
  providerAllowlist?: string;
}

export interface DistroBundleInfo {
  present: boolean;
  appVersion?: string;
  featureToggles?: Record<string, boolean>;
  security?: DistroSecurityManifest;
}
