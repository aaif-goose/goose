import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  FolderOpen,
  MessageSquarePlus,
  Pencil,
  Share2,
  Trash2,
} from "lucide-react";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import { DetailPageShell, PageHeader } from "@/shared/ui/page-shell";
import type { SkillInfo } from "../api/skills";

interface SkillDetailPageProps {
  skill: SkillInfo | null;
  onBack: () => void;
  onEdit: (skill: SkillInfo) => void;
  onReveal: (skill: SkillInfo) => void;
  onShare: (skill: SkillInfo) => void;
  onStartChat: (skill: SkillInfo) => void;
  onDelete: (skill: SkillInfo) => void;
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

  return (
    <DetailPageShell>
      <div className="space-y-5 border-b border-border pb-6">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="w-fit px-0 text-muted-foreground hover:text-foreground"
          onClick={onBack}
        >
          <ArrowLeft className="size-4" />
          {t("view.backToSkills")}
        </Button>

        <PageHeader
          title={
            <div>
              <div className="flex flex-wrap gap-2">
                {sourceLabels.map((label) => (
                  <Badge
                    key={label}
                    variant="secondary"
                    className="font-normal"
                  >
                    {label}
                  </Badge>
                ))}
              </div>
              <span className="mt-4 block text-2xl font-normal tracking-tight text-foreground">
                {skill.name}
              </span>
            </div>
          }
          titleElement="div"
          description={skill.description}
          descriptionClassName="max-w-3xl leading-relaxed"
          actions={
            <>
              <Button
                type="button"
                size="xs"
                variant="outline-flat"
                onClick={() => onStartChat(skill)}
              >
                <MessageSquarePlus className="size-3.5" />
                {t("view.startChat", { name: skill.name })}
              </Button>
              {skill.editable ? (
                <Button
                  type="button"
                  size="xs"
                  variant="outline-flat"
                  onClick={() => onEdit(skill)}
                >
                  <Pencil className="size-3.5" />
                  {t("common:actions.edit")}
                </Button>
              ) : null}
              <Button
                type="button"
                size="xs"
                variant="outline-flat"
                onClick={() => onShare(skill)}
              >
                <Share2 className="size-3.5" />
                {t("view.share")}
              </Button>
              <Button
                type="button"
                size="xs"
                variant="outline-flat"
                onClick={() => onReveal(skill)}
              >
                <FolderOpen className="size-3.5" />
                {t("view.reveal")}
              </Button>
            </>
          }
          className="items-start"
          actionsClassName="flex-wrap"
        />
      </div>

      <div className="grid gap-10 lg:grid-cols-[minmax(0,18rem)_minmax(0,1fr)]">
        <aside className="space-y-5">
          <section className="space-y-3 border-b border-border pb-5">
            <div>
              <p className="text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
                {t("view.source")}
              </p>
              <div className="mt-2 flex flex-wrap gap-2">
                {sourceLabels.map((label) => (
                  <Badge
                    key={label}
                    variant="secondary"
                    className="font-normal"
                  >
                    {label}
                  </Badge>
                ))}
              </div>
            </div>

            {skill.projectLinks.length > 0 ? (
              <div>
                <p className="text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
                  {t("view.projects")}
                </p>
                <div className="mt-2 space-y-1.5 text-sm text-foreground">
                  {skill.projectLinks.map((project) => (
                    <div key={`${project.id}-${project.workingDir}`}>
                      <p>{project.name}</p>
                      <p className="text-xs text-muted-foreground">
                        {project.workingDir}
                      </p>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            <div>
              <p className="text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
                {t("view.location")}
              </p>
              <p className="mt-2 text-sm text-foreground">
                {skill.directoryPath}
              </p>
            </div>

            <div>
              <p className="text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
                {t("view.filePath")}
              </p>
              <p className="mt-2 break-all text-sm text-muted-foreground">
                {skill.path}
              </p>
            </div>
          </section>
        </aside>

        <section className="space-y-3 pb-6">
          <div className="flex items-center justify-between gap-3 border-b border-border pb-4">
            <p className="text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
              {t("view.instructions")}
            </p>
            {skill.editable ? (
              <Button
                type="button"
                size="xs"
                variant="ghost-light"
                onClick={() => onDelete(skill)}
              >
                <Trash2 className="size-3.5" />
                {t("common:actions.delete")}
              </Button>
            ) : null}
          </div>
          <pre className="overflow-x-auto whitespace-pre-wrap text-sm leading-relaxed text-foreground">
            {skill.instructions || " "}
          </pre>
        </section>
      </div>
    </DetailPageShell>
  );
}
