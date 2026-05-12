import { useTranslation } from "react-i18next";
import { AtSign, Plus, Upload } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

interface SkillsEmptyStateProps {
  hasAnySkills: boolean;
  isDragOver: boolean;
  dropHandlers: React.HTMLAttributes<HTMLDivElement>;
  onNewSkill: () => void;
  onImport: () => void;
}

export function SkillsEmptyState({
  hasAnySkills,
  isDragOver,
  dropHandlers,
  onNewSkill,
  onImport,
}: SkillsEmptyStateProps) {
  const { t } = useTranslation(["skills", "common"]);

  return (
    <div
      {...dropHandlers}
      className={cn(
        "flex flex-col items-center justify-center gap-3 rounded-2xl py-16 text-muted-foreground transition-colors",
        isDragOver ? "bg-muted/40" : "border-transparent",
      )}
    >
      <AtSign className="h-10 w-10 opacity-30" />
      <div className="text-center">
        <p className="text-sm font-normal text-foreground">
          {hasAnySkills ? t("view.noMatchesTitle") : t("view.emptyTitle")}
        </p>
        <p className="mt-1 text-sm text-muted-foreground">
          {hasAnySkills
            ? t("view.noMatchesDescription")
            : t("view.emptyDescription")}
        </p>
      </div>
      {!hasAnySkills ? (
        <div className="mt-2 flex items-center gap-2">
          <Button
            type="button"
            variant="outline-flat"
            size="xs"
            onClick={onNewSkill}
          >
            <Plus className="size-3.5" />
            {t("view.newSkill")}
          </Button>
          <Button
            type="button"
            variant="outline-flat"
            size="xs"
            onClick={onImport}
          >
            <Upload className="size-3.5" />
            {t("common:actions.import")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
