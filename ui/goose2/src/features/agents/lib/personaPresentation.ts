import type { Persona } from "@/shared/types/agents";

export type PersonaSource = "builtin" | "file" | "custom";

export function getPersonaSource(persona: Persona): PersonaSource {
  if (persona.isBuiltin) {
    return "builtin";
  }
  if (persona.isFromDisk) {
    return "file";
  }
  return "custom";
}

export function isPersonaReadOnly(persona: Persona): boolean {
  return getPersonaSource(persona) !== "custom";
}
