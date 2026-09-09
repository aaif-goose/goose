import type { RequestPermissionRequest } from '@agentclientprotocol/sdk';

export interface AcpPermissionRequest {
  generation: string;
  request: RequestPermissionRequest;
}
