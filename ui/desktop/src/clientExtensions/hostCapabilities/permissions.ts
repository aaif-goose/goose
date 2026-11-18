export const HOST_PERMISSIONS = [
  'platform:read',
  'providers:read',
  'providers:write',
  'sessions:read',
  'sessions:events',
  'sessions:create',
  'recipes:read',
  'commands:execute',
  'storage:readwrite',
  'schedules:read',
  'schedules:manage',
  'tools:read',
  'tools:call',
  'net:fetch',
] as const;

export type HostPermission = (typeof HOST_PERMISSIONS)[number];

export function isHostPermission(value: unknown): value is HostPermission {
  return typeof value === 'string' && (HOST_PERMISSIONS as readonly string[]).includes(value);
}
