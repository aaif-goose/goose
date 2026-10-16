import { z } from 'zod';
import { acpFetchClientExtensionNet } from '../../../acp/clientExtensions';
import { parsePayload } from '../payload';
import type { HostCapabilityDefinition } from '../types';

const MAX_BODY_LENGTH = 512 * 1024;
const ALLOWED_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD'] as const;

const fetchPayload = z.object({
  url: z.string().trim().min(1),
  method: z.enum(ALLOWED_METHODS).default('GET'),
  headers: z.record(z.string(), z.string()).optional(),
  body: z.string().max(MAX_BODY_LENGTH).optional(),
});

export const netPower: HostCapabilityDefinition = {
  id: 'net',
  description: 'Fetch a URL whose origin the manifest explicitly allow-lists.',
  methods: {
    fetch: {
      permission: 'net:fetch',
      handle: async (context, payload) => {
        const { url, method, headers, body } = parsePayload(fetchPayload, payload);
        let parsed: URL;
        try {
          parsed = new URL(url);
        } catch {
          throw new Error(`Invalid URL "${url}"`);
        }
        if (!context.allowedOrigins.includes(parsed.origin)) {
          throw new Error(
            `Plugin "${context.extensionId}" has not allow-listed origin "${parsed.origin}" in its manifest's "network" field`
          );
        }
        return acpFetchClientExtensionNet(context.extensionId, url, method, headers, body);
      },
    },
  },
};
