import { COMMON_HOST_POWERS } from './powers';
import type { HostCapabilityDefinition, HostMethodDefinition } from './types';

const HOST_CAPABILITY_REGISTRY: Record<string, HostCapabilityDefinition> = Object.fromEntries(
  COMMON_HOST_POWERS.map((power) => [power.id, power])
);

function ownEntry<T>(record: Record<string, T>, key: string): T | undefined {
  return Object.prototype.hasOwnProperty.call(record, key) ? record[key] : undefined;
}

export function findHostCapability(id: string): HostCapabilityDefinition | undefined {
  return ownEntry(HOST_CAPABILITY_REGISTRY, id);
}

export function findHostMethod(
  capability: string,
  method: string
): HostMethodDefinition | undefined {
  const definition = findHostCapability(capability);
  return definition ? ownEntry(definition.methods, method) : undefined;
}
