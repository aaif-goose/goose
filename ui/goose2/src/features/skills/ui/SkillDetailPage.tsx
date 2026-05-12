import type { ButtonHTMLAttributes, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  IconDots,
  IconFolderOpen,
  IconMessagePlus,
  IconPencil,
  IconShare,
  IconTrash,
} from "@tabler/icons-react";
import { MessageResponse } from "@/shared/ui/ai-elements/message";
import { Button } from "@/shared/ui/button";
import { DetailField } from "@/shared/ui/detail-field";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { PageColumns } from "@/shared/ui/page-columns";
import { DetailPageShell, PageHeader } from "@/shared/ui/page-shell";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import type { SkillInfo } from "../api/skills";

interface SkillDetailPageProps {
  skill: SkillInfo | null;
  onBack: () => void;
  onEdit: (skill: SkillInfo) => void;
  onReveal: (skill: SkillInfo) => void;
  onShare: (skill: SkillInfo) => void;
  onStartChat?: (skill: SkillInfo) => void;
  onDelete: (skill: SkillInfo) => void;
}

interface SkillHeaderActionButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  icon: ReactNode;
  tooltipSide?: "top" | "right" | "bottom" | "left";
}

function SkillHeaderActionButton({
  label,
  icon,
  type = "button",
  tooltipSide = "top",
  ...props
}: SkillHeaderActionButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type={type}
          size="icon-xs"
          variant="outline-flat"
          aria-label={label}
          {...props}
        >
          {icon}
          <span className="sr-only">{label}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent side={tooltipSide} align="center" sideOffset={8}>
        <p>{label}</p>
      </TooltipContent>
    </Tooltip>
  );
}

export function SkillDetailPage({
  skill,
  onBack,
  onEdit,
  onReveal,
  onShare,
  onStartChat,
  onDelete,
}: SkillDetailPageProps) {
  const { t } = useTranslation(["skills", "common"]);

  if (!skill) {
    return (
      <div className="flex h-full flex-col justify-center px-1 text-sm text-muted-foreground">
        <p className="text-sm text-foreground">{t("view.detailEmptyTitle")}</p>
        <p className="mt-1 text-sm text-muted-foreground">
          {t("view.detailEmptyDescription")}
        </p>
      </div>
    );
  }

  const sourceLabels =
    skill.projectLinks.length > 0
      ? [...new Set(skill.projectLinks.map((project) => project.name))]
      : [skill.sourceLabel];
  const startChatLabel = t("view.startChatShort");
  const editLabel = t("common:actions.edit");
  const revealLabel = t("view.reveal");
  const moreLabel = t("view.more");
  const isBuiltin = skill.sourceKind === "builtin";
  const actions = (
    <>
      {onStartChat ? (
        <SkillHeaderActionButton
          label={startChatLabel}
          icon={<IconMessagePlus className="size-3.5" />}
          tooltipSide="top"
          onClick={() => onStartChat(skill)}
        />
      ) : null}
      {!isBuiltin ? (
        <>
          <SkillHeaderActionButton
            label={editLabel}
            icon={<IconPencil className="size-3.5" />}
            tooltipSide="top"
            onClick={() => onEdit(skill)}
          />
          <SkillHeaderActionButton
            label={revealLabel}
            icon={<IconFolderOpen className="size-3.5" />}
            tooltipSide="top"
            onClick={() => onReveal(skill)}
          />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="icon-xs"
                variant="outline-flat"
                aria-label={moreLabel}
              >
                <IconDots className="size-3.5" />
                <span className="sr-only">{moreLabel}</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" sideOffset={8}>
              <DropdownMenuItem onSelect={() => onShare(skill)}>
                <IconShare className="size-3.5" />
                {t("view.share")}
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => onDelete(skill)}
              >
                <IconTrash className="size-3.5" />
                {t("common:actions.delete")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </>
      ) : null}
    </>
  );

  return (
    <DetailPageShell>
      <div className="space-y-5 border-b border-border pb-6">
        <Button
          type="button"
          variant="back"
          size="sm"
          className="w-fit"
          onClick={onBack}
        >
          {t("view.backToSkills")}
        </Button>

        <PageHeader
          title={skill.name}
          variant="detail"
          description={skill.description}
          actionsPlacement="below"
          descriptionClassName="max-w-3xl leading-relaxed"
          actions={actions}
          actionsClassName="gap-2"
        />
      </div>

      {isBuiltin ? (
        <section className="space-y-4 pb-6">
          <DetailField label={t("view.instructions")} />
          <MessageResponse className="min-w-0 text-sm leading-6">
            {skill.instructions || " "}
          </MessageResponse>
        </section>
      ) : (
        <PageColumns
          defaultSidebarSize={28}
          minSidebarSize={22}
          maxSidebarSize={36}
          minContentSize={52}
          sidebar={
            <aside className="space-y-5">
              <section className="space-y-5 border-b border-border pb-5">
                <DetailField
                  label={t("view.source")}
                  contentClassName="space-y-1 text-foreground"
                >
                  {sourceLabels.map((label) => (
                    <p key={label}>{label}</p>
                  ))}
                </DetailField>

                {skill.projectLinks.length > 0 ? (
                  <DetailField
                    label={t("view.projects")}
                    contentClassName="space-y-1.5"
                  >
                    {skill.projectLinks.map((project) => (
                      <div key={`${project.id}-${project.workingDir}`}>
                        <p>{project.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {project.workingDir}
                        </p>
                      </div>
                    ))}
                  </DetailField>
                ) : null}

                <DetailField
                  label={t("view.location")}
                  contentAs="p"
                  contentClassName="break-all text-foreground"
                >
                  {skill.fileLocation}
                </DetailField>
              </section>
            </aside>
          }
        >
          <section className="space-y-4 pb-6">
            <DetailField label={t("view.instructions")} />
            <MessageResponse className="min-w-0 text-sm leading-6">
              {skill.instructions || " "}
            </MessageResponse>
          </section>
        </PageColumns>
      )}
    </DetailPageShell>
  );
}
