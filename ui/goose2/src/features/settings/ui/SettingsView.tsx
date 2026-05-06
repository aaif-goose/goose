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

interface SettingsViewProps {
  activeSection: SectionId;
}

export function SettingsView({ activeSection }: SettingsViewProps) {
  return (
    <div className="h-full overflow-y-auto">
      <div className="min-h-full px-6 pb-4">
        {activeSection === "appearance" && <AppearanceSettings />}
        {activeSection === "providers" && <ProvidersSettings />}
        {activeSection === "compaction" && <CompactionSettings />}
        {activeSection === "voice" && <VoiceInputSettings />}
        {activeSection === "doctor" && <DoctorSettings />}
        {activeSection === "general" && <GeneralSettings />}
        {activeSection === "projects" && <ProjectsSettings />}
        {activeSection === "chats" && <ChatsSettings />}
        {activeSection === "about" && <AboutSettings />}
      </div>
    </div>
  );
}
