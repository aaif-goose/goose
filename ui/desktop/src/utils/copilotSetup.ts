import { disconnect, getStatus, setup as setupCopilotApi } from '../api/sdk.gen';

export async function startCopilotSetup(): Promise<{ installation_id: number }> {
  const { data } = await setupCopilotApi({ throwOnError: true });
  if (!data?.installation_id) {
    throw new Error('setup returned no installation_id');
  }
  return data;
}

export async function fetchCopilotInstallId(): Promise<number | null> {
  const { data } = await getStatus({ throwOnError: true });
  const id = data?.installation_id;
  return typeof id === 'number' && Number.isFinite(id) ? id : null;
}

export async function disconnectCopilot(): Promise<void> {
  await disconnect({ throwOnError: true });
}
