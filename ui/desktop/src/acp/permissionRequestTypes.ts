import type { RequestPermissionRequest } from '@agentclientprotocol/sdk/unstable';

export interface AcpPermissionRequest {
  generation: string;
  request: RequestPermissionRequest;
}
