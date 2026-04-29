export interface DistroSecurityManifest {
  extensionAllowlist?: string;
  providerAllowlist?: string;
}

export interface DistroBundleInfo {
  present: boolean;
  version?: string;
  security?: DistroSecurityManifest;
}
