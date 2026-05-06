import type { ComponentType } from "react";
import {
  Mic,
  Minimize2,
  Palette,
  Settings2,
  FolderKanban,
  Info,
  MessageSquare,
  Stethoscope,
} from "lucide-react";
import { IconPlug } from "@tabler/icons-react";

export const SETTINGS_SECTIONS = [
  { id: "appearance", labelKey: "nav.appearance", icon: Palette },
  { id: "providers", labelKey: "nav.providers", icon: IconPlug },
  { id: "compaction", labelKey: "nav.compaction", icon: Minimize2 },
  { id: "voice", labelKey: "nav.voice", icon: Mic },
  { id: "general", labelKey: "nav.general", icon: Settings2 },
  { id: "projects", labelKey: "nav.projects", icon: FolderKanban },
  { id: "chats", labelKey: "nav.chats", icon: MessageSquare },
  { id: "doctor", labelKey: "nav.doctor", icon: Stethoscope },
  { id: "about", labelKey: "nav.about", icon: Info },
] as const satisfies readonly {
  id: string;
  labelKey: string;
  icon: ComponentType<{ className?: string }>;
}[];

export type SectionId = (typeof SETTINGS_SECTIONS)[number]["id"];

export const DEFAULT_SETTINGS_SECTION: SectionId = "appearance";

export function isSettingsSection(section: string): section is SectionId {
  return SETTINGS_SECTIONS.some((item) => item.id === section);
}
