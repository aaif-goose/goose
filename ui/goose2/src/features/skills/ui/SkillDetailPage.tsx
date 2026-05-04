import type { ButtonHTMLAttributes, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  Copy,
  CopyPlus,
  MessageSquarePlus,
  MoreVertical,
  Pencil,
  Save,
  Share2,
  Trash2,
} from "lucide-react";
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
import type { SkillInfo } from "../api/skills";
import type { SkillViewInfo } from "../lib/skillCategories";

interface SkillDetailPageProps {
  skill: SkillViewInfo | null;
  onBack: () => void;
  onEdit: (skill: SkillInfo) => void;
  onCopyFile: (skill: SkillInfo) => void;
  onSaveCopy: (skill: SkillInfo) => void;
  onStartChat?: (skill: SkillInfo) => void;
  onDuplicate: (skill: SkillInfo) => void;
  onDelete: (skill: SkillInfo) => void;
}

interface SkillHeaderActionButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  icon: ReactNode;
}

function SkillHeaderActionButton({
  label,
  icon,
  type = "button",
  ...props
}: SkillHeaderActionButtonProps) {
  return (
    <Button
      type={type}
      size="xs"
      variant="outline-flat"
      leftIcon={icon}
      {...props}
    >
      {label}
    </Button>
  );
}

export function SkillDetailPage({
  skill,
  onBack,
  onEdit,
  onCopyFile,
  onSaveCopy,
  onStartChat,
  onDuplicate,
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
  const shareLabel = t("view.share");
  const moreLabel = t("view.more");

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
          actions={
            <>
              {onStartChat ? (
                <SkillHeaderActionButton
                  label={startChatLabel}
                  icon={<MessageSquarePlus aria-hidden="true" />}
                  onClick={() => onStartChat(skill)}
                />
              ) : null}
              <SkillHeaderActionButton
                label={editLabel}
                icon={<Pencil aria-hidden="true" />}
                onClick={() => onEdit(skill)}
              />
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    size="xs"
                    variant="outline-flat"
                    leftIcon={<Share2 aria-hidden="true" />}
                  >
                    {shareLabel}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" sideOffset={8}>
                  <DropdownMenuItem onSelect={() => onCopyFile(skill)}>
                    <Copy className="size-3.5" />
                    {t("view.copyFile")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => onSaveCopy(skill)}>
                    <Save className="size-3.5" />
                    {t("view.saveCopy")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    size="icon-xs"
                    variant="outline-flat"
                    aria-label={moreLabel}
                  >
                    <MoreVertical className="size-3.5" />
                    <span className="sr-only">{moreLabel}</span>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" sideOffset={8}>
                  <DropdownMenuItem onSelect={() => onDuplicate(skill)}>
                    <CopyPlus className="size-3.5" />
                    {t("common:actions.duplicate")}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    variant="destructive"
                    onSelect={() => onDelete(skill)}
                  >
                    <Trash2 className="size-3.5" />
                    {t("common:actions.delete")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          }
          actionsClassName="gap-2"
        />
      </div>

      <PageColumns
        defaultSidebarSize={28}
        minSidebarSize={22}
        maxSidebarSize={36}
        minContentSize={52}
        sidebar={
          <aside className="space-y-5">
            <section className="space-y-5 border-b border-border pb-5">
              <DetailField
                label={t("view.category")}
                contentAs="p"
                contentClassName="text-foreground"
              >
                {t(`view.categories.options.${skill.inferredCategory}`)}
              </DetailField>

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
    </DetailPageShell>
  );
}
