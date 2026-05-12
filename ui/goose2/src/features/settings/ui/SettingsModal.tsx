import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  Mic,
  Minimize2,
  Palette,
  Settings2,
  FolderKanban,
  Info,
  MessageSquare,
  Stethoscope,
  X,
} from "lucide-react";
import { IconPlug } from "@tabler/icons-react";
import { AppearanceSettings } from "./AppearanceSettings";
import { DoctorSettings } from "./DoctorSettings";
import { ProvidersSettings } from "./ProvidersSettings";
import { VoiceInputSettings } from "./VoiceInputSettings";
import { GeneralSettings } from "./GeneralSettings";
import { CompactionSettings } from "./CompactionSettings";
import { ProjectsSettings } from "./ProjectsSettings";
import { ChatsSettings } from "./ChatsSettings";
import { AboutSettings } from "./AboutSettings";

const NAV_ITEMS = [
  { id: "appearance", labelKey: "nav.appearance", icon: Palette },
  { id: "providers", labelKey: "nav.providers", icon: IconPlug },
  { id: "compaction", labelKey: "nav.compaction", icon: Minimize2 },
  { id: "voice", labelKey: "nav.voice", icon: Mic },
  { id: "general", labelKey: "nav.general", icon: Settings2 },
  { id: "projects", labelKey: "nav.projects", icon: FolderKanban },
  { id: "chats", labelKey: "nav.chats", icon: MessageSquare },
  { id: "doctor", labelKey: "nav.doctor", icon: Stethoscope },
  { id: "about", labelKey: "nav.about", icon: Info },
] as const;

export type SectionId = (typeof NAV_ITEMS)[number]["id"];

interface SettingsModalProps {
  onClose: () => void;
  initialSection?: SectionId;
}

export function SettingsModal({
  onClose,
  initialSection = "appearance",
}: SettingsModalProps) {
  const { t } = useTranslation(["settings", "common"]);
  const [activeSection, setActiveSection] = useState<SectionId>(initialSection);
  const [isLoaded, setIsLoaded] = useState(false);
  const modalRootRef = useRef<HTMLDivElement>(null);

  // Trigger entrance animations after mount
  useEffect(() => {
    const timer = setTimeout(() => setIsLoaded(true), 50);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    setActiveSection(initialSection);
  }, [initialSection]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !event.defaultPrevented &&
        event.target instanceof Node &&
        modalRootRef.current?.contains(event.target)
      ) {
        onClose();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const navItems = NAV_ITEMS.map((item) => ({
    ...item,
    label: t(item.labelKey),
  }));
  const activeSectionLabel =
    navItems.find((item) => item.id === activeSection)?.label ?? t("title");

  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: Escape is handled by the document listener while the backdrop only handles pointer dismissal.
    <div
      ref={modalRootRef}
      role="dialog"
      aria-modal="true"
      aria-label={activeSectionLabel}
      className={cn(
        "fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm transition-opacity duration-300",
        isLoaded ? "opacity-100" : "opacity-0",
      )}
      onClick={onClose}
    >
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: stopPropagation on inner container is not a meaningful interaction */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: click handler only prevents backdrop dismiss propagation */}
      <div
        className={cn(
          "flex h-[min(600px,calc(100vh-4rem))] w-[calc(100vw-2rem)] max-w-3xl overflow-hidden rounded-xl border bg-background shadow-modal transition-opacity duration-300 ease-out max-[640px]:h-[calc(100vh-1rem)] max-[640px]:w-[calc(100vw-1rem)] max-[640px]:flex-col",
          isLoaded ? "opacity-100" : "opacity-0",
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Sidebar */}
        <div
          className={cn(
            "flex min-h-0 w-44 flex-shrink-0 flex-col border-r bg-muted/50 transition-all duration-700 ease-out max-[640px]:max-h-32 max-[640px]:w-full max-[640px]:border-b max-[640px]:border-r-0",
            isLoaded ? "opacity-100 translate-x-0" : "opacity-0 -translate-x-2",
          )}
        >
          <div
            className={cn(
              "px-4 py-4 transition-all duration-500 ease-out max-[640px]:px-3 max-[640px]:py-3",
              isLoaded
                ? "opacity-100 translate-x-0"
                : "opacity-0 -translate-x-2",
            )}
          >
            <h2 className="text-sm font-semibold">{t("title")}</h2>
          </div>
          <nav className="min-h-0 flex-1 overflow-y-auto px-2 pb-3 max-[640px]:overflow-x-auto max-[640px]:overflow-y-hidden max-[640px]:pb-2">
            <div className="flex flex-col gap-1 max-[640px]:min-w-max max-[640px]:flex-row">
              {navItems.map((item, index) => (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  key={item.id}
                  onClick={() => setActiveSection(item.id)}
                  className={cn(
                    "w-full justify-start rounded-lg px-3 py-2 transition-all duration-600 ease-out max-[640px]:w-auto max-[640px]:shrink-0 max-[640px]:px-2.5",
                    activeSection === item.id
                      ? "bg-background text-foreground shadow-none hover:bg-background"
                      : "text-muted-foreground hover:bg-accent/50 hover:text-foreground duration-300",
                    isLoaded
                      ? "opacity-100 translate-x-0"
                      : "opacity-0 translate-x-4",
                  )}
                  style={{
                    transitionDelay: isLoaded ? "0ms" : `${index * 40 + 300}ms`,
                  }}
                >
                  <item.icon className="size-4" />
                  {item.label}
                </Button>
              ))}
            </div>
          </nav>
        </div>

        {/* Content */}
        <div className="relative flex min-w-0 flex-1 flex-col">
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            onClick={onClose}
            aria-label={t("common:actions.close")}
            className="absolute right-4 top-4 z-30 rounded-md text-muted-foreground hover:text-foreground max-[640px]:right-3 max-[640px]:top-3"
          >
            <X className="size-4" />
          </Button>

          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="px-6 pb-4 max-[640px]:px-4 max-[640px]:pt-1">
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
        </div>
      </div>
    </div>
  );
}
