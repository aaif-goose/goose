export interface ModelOption {
  id: string;
  name: string;
  displayName?: string;
  provider?: string;
  providerId?: string;
  providerName?: string;
  /** Whether this model should appear in the compact recommended picker. */
  recommended?: boolean;
}
