import { IconLoader2, IconUpload } from "@tabler/icons-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import type { ProjectIconCandidate } from "../api/projects";
import { PROJECT_TABLER_ICONS, isImageProjectIcon } from "../lib/projectIcons";
import { ProjectIcon } from "./ProjectIcon";

interface ProjectIconPickerProps {
  icon: string;
  iconCandidates: ProjectIconCandidate[];
  iconScanPending: boolean;
  error?: string | null;
  onChooseIcon: (icon: string) => void;
  onChooseCustomIcon: () => void;
}

export function ProjectIconPicker({
  icon,
  iconCandidates,
  iconScanPending,
  error,
  onChooseIcon,
  onChooseCustomIcon,
}: ProjectIconPickerProps) {
  const { t } = useTranslation("projects");
  const selectedCustomIcon =
    isImageProjectIcon(icon) &&
    !iconCandidates.some((candidate) => candidate.icon === icon);

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-3">
        <span className="text-xs font-medium text-muted-foreground">
          {t("dialog.icon")}
        </span>
        {iconScanPending && (
          <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
            <IconLoader2 className="size-3 animate-spin" />
            {t("dialog.scanningIcons")}
          </span>
        )}
      </div>
      <div className="max-h-36 overflow-y-auto rounded-md border border-border bg-muted/20 p-2">
        <div className="grid grid-cols-[repeat(auto-fit,minmax(2.5rem,1fr))] justify-items-center gap-2">
          {iconCandidates.map((candidate) => (
            <button
              key={candidate.id}
              type="button"
              onClick={() => onChooseIcon(candidate.icon)}
              className={cn(
                "flex size-9 items-center justify-center rounded-md border bg-background transition-colors hover:bg-muted",
                icon === candidate.icon
                  ? "border-foreground"
                  : "border-border-soft",
              )}
              title={t("dialog.iconCandidateTitle", {
                sourceDir: candidate.sourceDir,
                label: candidate.label,
              })}
              aria-label={t("dialog.iconAria", { icon: candidate.label })}
            >
              <ProjectIcon
                icon={candidate.icon}
                imageClassName="size-5 rounded-[4px]"
              />
            </button>
          ))}
          {PROJECT_TABLER_ICONS.map((tablerIcon) => {
            const label = t(tablerIcon.labelKey);
            return (
              <button
                key={tablerIcon.value}
                type="button"
                onClick={() => onChooseIcon(tablerIcon.value)}
                className={cn(
                  "flex size-9 items-center justify-center rounded-md border bg-background text-foreground transition-colors hover:bg-muted",
                  icon === tablerIcon.value
                    ? "border-foreground"
                    : "border-border-soft",
                )}
                title={label}
                aria-label={t("dialog.iconAria", { icon: label })}
              >
                <ProjectIcon icon={tablerIcon.value} />
              </button>
            );
          })}
          <button
            type="button"
            onClick={onChooseCustomIcon}
            className={cn(
              "col-span-2 flex h-9 min-w-[88px] items-center justify-center gap-1.5 rounded-md border bg-background px-3 text-xs text-foreground transition-colors hover:bg-muted",
              selectedCustomIcon ? "border-foreground" : "border-border-soft",
            )}
            title={
              selectedCustomIcon
                ? t("dialog.customIcon")
                : t("dialog.uploadIcon")
            }
            aria-label={t("dialog.customIcon")}
          >
            {selectedCustomIcon ? (
              <ProjectIcon icon={icon} imageClassName="size-4 rounded-[3px]" />
            ) : (
              <IconUpload className="size-3.5" />
            )}
            <span>{t("dialog.uploadIcon")}</span>
          </button>
        </div>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}
