import { AppearanceSettings } from "./AppearanceSettings";
import { DoctorSettings } from "./DoctorSettings";
import { ProvidersSettings } from "./ProvidersSettings";
import { VoiceInputSettings } from "./VoiceInputSettings";
import { GeneralSettings } from "./GeneralSettings";
import { CompactionSettings } from "./CompactionSettings";
import { ProjectsSettings } from "./ProjectsSettings";
import { ChatsSettings } from "./ChatsSettings";
import { AboutSettings } from "./AboutSettings";
import type { SectionId } from "./settingsSections";
import { PageShell } from "@/shared/ui/page-shell";

interface SettingsViewProps {
  activeSection: SectionId;
}

export function SettingsView({ activeSection }: SettingsViewProps) {
  return (
    <PageShell contentClassName="gap-0">
      {activeSection === "appearance" && <AppearanceSettings />}
      {activeSection === "providers" && <ProvidersSettings />}
      {activeSection === "compaction" && <CompactionSettings />}
      {activeSection === "voice" && <VoiceInputSettings />}
      {activeSection === "doctor" && <DoctorSettings />}
      {activeSection === "general" && <GeneralSettings />}
      {activeSection === "projects" && <ProjectsSettings />}
      {activeSection === "chats" && <ChatsSettings />}
      {activeSection === "about" && <AboutSettings />}
    </PageShell>
  );
}
