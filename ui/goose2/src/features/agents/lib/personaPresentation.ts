import type { Persona } from "@/shared/types/agents";

export type PersonaSource = "builtin" | "file" | "custom";

export function getPersonaSource(persona: Persona): PersonaSource {
  if (persona.isBuiltin) {
    return "builtin";
  }
  if (persona.writable === true) {
    return "custom";
  }
  if (persona.isFromDisk) {
    return "file";
  }
  return "custom";
}

export function isPersonaReadOnly(persona: Persona): boolean {
  return persona.writable === false || getPersonaSource(persona) !== "custom";
}
