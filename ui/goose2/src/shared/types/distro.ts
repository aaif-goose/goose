export interface DistroSecurityManifest {
  extensionAllowlist?: string;
  providerAllowlist?: string;
}

export interface DistroBundleInfo {
  present: boolean;
  featureToggles?: Record<string, boolean>;
  security?: DistroSecurityManifest;
}
