import { z } from 'zod';
import { acpListProviderDetails, acpReadDefaults, acpSaveDefaults } from '../../../acp/providers';
import { parsePayload } from '../payload';
import type { HostCapabilityDefinition } from '../types';

const setDefaultPayload = z.object({
  providerId: z.string().trim().min(1),
  modelId: z.string().trim().min(1).nullish(),
});

export const providersPower: HostCapabilityDefinition = {
  id: 'providers',
  description: 'Read the provider inventory and the default provider and model.',
  methods: {
    list: {
      permission: 'providers:read',
      handle: async () => {
        const providers = await acpListProviderDetails();
        return providers.map((provider) => ({
          id: provider.name,
          displayName: provider.metadata.display_name,
          configured: provider.is_configured,
          available: provider.is_available,
          defaultModel: provider.metadata.default_model,
        }));
      },
    },
    getDefault: {
      permission: 'providers:read',
      handle: () => acpReadDefaults(),
    },
    setDefault: {
      permission: 'providers:write',
      handle: async (_context, payload) => {
        const { providerId, modelId = null } = parsePayload(setDefaultPayload, payload);
        await acpSaveDefaults(providerId, modelId);
        return { providerId, modelId };
      },
    },
  },
};
