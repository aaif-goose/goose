import type { HostCapabilityDefinition } from '../types';

export const platformPower: HostCapabilityDefinition = {
  id: 'platform',
  description: 'OS and runtime info about the client running the plugin.',
  methods: {
    getInfo: {
      permission: 'platform:read',
      handle: () => ({
        platform: window.electron.platform,
        arch: window.electron.arch,
      }),
    },
  },
};
