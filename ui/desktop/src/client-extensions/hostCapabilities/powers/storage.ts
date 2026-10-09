import { z } from 'zod';
import {
  acpDeleteClientExtensionStorage,
  acpGetClientExtensionStorage,
  acpListClientExtensionStorageKeys,
  acpSetClientExtensionStorage,
} from '../../../acp/clientExtensions';
import { parsePayload } from '../payload';
import type { HostCapabilityDefinition } from '../types';

const keyPayload = z.object({ key: z.string().min(1).max(200) });
const setPayload = keyPayload.extend({ value: z.json() });

export const storagePower: HostCapabilityDefinition = {
  id: 'storage',
  description: 'Persist small JSON values for the plugin across restarts.',
  methods: {
    get: {
      permission: 'storage:readwrite',
      handle: async (context, payload) => {
        const { key } = parsePayload(keyPayload, payload);
        return acpGetClientExtensionStorage(context.extensionId, key);
      },
    },
    set: {
      permission: 'storage:readwrite',
      handle: async (context, payload) => {
        const { key, value } = parsePayload(setPayload, payload);
        await acpSetClientExtensionStorage(context.extensionId, key, value);
        return { key };
      },
    },
    delete: {
      permission: 'storage:readwrite',
      handle: async (context, payload) => {
        const { key } = parsePayload(keyPayload, payload);
        const existed = await acpDeleteClientExtensionStorage(context.extensionId, key);
        return { key, existed };
      },
    },
    keys: {
      permission: 'storage:readwrite',
      handle: (context) => acpListClientExtensionStorageKeys(context.extensionId),
    },
  },
};
