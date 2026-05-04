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
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionSectionTrigger,
} from "@/shared/ui/accordion";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import type { SkillViewInfo } from "../lib/skillCategories";
import type { SkillsSection } from "../lib/skillsHelpers";

interface SkillsListSectionsProps {
  sections: SkillsSection[];
  expandedSectionIds: string[];
  onExpandedSectionIdsChange: (ids: string[]) => void;
  onSelectSkill: (skill: SkillViewInfo) => void;
  onStartChat?: (skill: SkillViewInfo) => void;
  onEdit: (skill: SkillViewInfo) => void;
  onCopyFile: (skill: SkillViewInfo) => void;
  onSaveCopy: (skill: SkillViewInfo) => void;
  onDuplicate: (skill: SkillViewInfo) => void;
  onDelete: (skill: SkillViewInfo) => void;
}

export function SkillsListSections({
  sections,
  expandedSectionIds,
  onExpandedSectionIdsChange,
  onSelectSkill,
  onStartChat,
  onEdit,
  onCopyFile,
  onSaveCopy,
  onDuplicate,
  onDelete,
}: SkillsListSectionsProps) {
  const { t } = useTranslation(["skills", "common"]);

  return (
    <Accordion
      type="multiple"
      value={expandedSectionIds}
      onValueChange={onExpandedSectionIdsChange}
      className="min-h-0 space-y-6"
    >
      {sections.map((section) => (
        <AccordionItem
          key={section.id}
          value={section.id}
          className="group/skills-section overflow-hidden rounded-2xl !border !border-border-soft bg-background"
        >
          <AccordionSectionTrigger
            title={section.title}
            meta={t("view.skillCount", {
              count: section.skills.length,
              displayCount: section.skills.length,
            })}
          />

          <AccordionContent className="pb-0">
            <div className="motion-safe:group-data-[state=closed]/skills-section:animate-accordion-content-close motion-safe:group-data-[state=open]/skills-section:animate-accordion-content-open border-t border-border-soft-divider will-change-[opacity,transform]">
              <div className="divide-y divide-border-soft-divider">
                {section.skills.map((skill) => (
                  <div
                    key={`${section.id}-${skill.id}`}
                    className="group relative px-5 py-4 transition-colors hover:bg-muted/20"
                  >
                    <button
                      type="button"
                      className="absolute inset-0 cursor-pointer outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring"
                      onClick={() => onSelectSkill(skill)}
                      aria-label={t("view.openDetails", { name: skill.name })}
                    />
                    <div className="pointer-events-none relative z-10 min-w-0">
                      <p className="pr-10 text-sm font-normal text-foreground">
                        {skill.name}
                      </p>
                      {skill.description ? (
                        <p className="mt-1 line-clamp-2 text-xs font-light text-muted-foreground">
                          {skill.description}
                        </p>
                      ) : null}
                    </div>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-xs"
                          className="absolute top-3 right-4 z-20 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 data-[state=open]:opacity-100"
                          onClick={(event) => event.stopPropagation()}
                          aria-label={t("view.optionsAria", {
                            name: skill.name,
                          })}
                        >
                          <MoreVertical className="size-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" sideOffset={4}>
                        {onStartChat ? (
                          <DropdownMenuItem onSelect={() => onStartChat(skill)}>
                            <MessageSquarePlus className="size-3.5" />
                            {t("view.startChatShort")}
                          </DropdownMenuItem>
                        ) : null}
                        <DropdownMenuItem onSelect={() => onEdit(skill)}>
                          <Pencil className="size-3.5" />
                          {t("common:actions.edit")}
                        </DropdownMenuItem>
                        <DropdownMenuSub>
                          <DropdownMenuSubTrigger>
                            <Share2 className="size-3.5" />
                            {t("view.share")}
                          </DropdownMenuSubTrigger>
                          <DropdownMenuSubContent>
                            <DropdownMenuItem
                              onSelect={() => onCopyFile(skill)}
                            >
                              <Copy className="size-3.5" />
                              {t("view.copyFile")}
                            </DropdownMenuItem>
                            <DropdownMenuItem
                              onSelect={() => onSaveCopy(skill)}
                            >
                              <Save className="size-3.5" />
                              {t("view.saveCopy")}
                            </DropdownMenuItem>
                          </DropdownMenuSubContent>
                        </DropdownMenuSub>
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
                  </div>
                ))}
              </div>
            </div>
          </AccordionContent>
        </AccordionItem>
      ))}
    </Accordion>
  );
}
