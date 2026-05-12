import { useTranslation } from "react-i18next";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionSectionTrigger,
} from "@/shared/ui/accordion";
import { Button } from "@/shared/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import { IconMessagePlus } from "@tabler/icons-react";
import type { SkillInfo } from "../api/skills";
import type { SkillsSection } from "../lib/skillsHelpers";

interface SkillsListSectionsProps {
  sections: SkillsSection[];
  expandedSectionIds: string[];
  onExpandedSectionIdsChange: (ids: string[]) => void;
  onSelectSkill: (skill: SkillInfo) => void;
  onStartChat?: (skill: SkillInfo) => void;
}

export function SkillsListSections({
  sections,
  expandedSectionIds,
  onExpandedSectionIdsChange,
  onSelectSkill,
  onStartChat,
}: SkillsListSectionsProps) {
  const { t } = useTranslation(["skills"]);

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
                    className="group relative flex items-center gap-3 px-5 py-4 transition-colors hover:bg-muted/20"
                  >
                    <button
                      type="button"
                      className="absolute inset-0 cursor-pointer outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring"
                      onClick={() => onSelectSkill(skill)}
                      aria-label={t("view.openDetails", { name: skill.name })}
                    />
                    <div className="pointer-events-none relative z-10 min-w-0 flex-1">
                      <p className="text-sm font-normal text-foreground">
                        {skill.name}
                      </p>
                      {skill.description ? (
                        <p className="mt-1 line-clamp-2 text-xs font-light text-muted-foreground">
                          {skill.description}
                        </p>
                      ) : null}
                    </div>
                    {onStartChat ? (
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button
                            type="button"
                            variant="outline-flat"
                            size="icon-xs"
                            className="relative z-20 shrink-0 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
                            onClick={() => onStartChat(skill)}
                            aria-label={t("view.startChat", {
                              name: skill.name,
                            })}
                          >
                            <IconMessagePlus className="size-3.5" />
                            <span className="sr-only">
                              {t("view.startChatShort")}
                            </span>
                          </Button>
                        </TooltipTrigger>
                        <TooltipContent
                          side="top"
                          align="center"
                          sideOffset={8}
                        >
                          <p>{t("view.startChatShort")}</p>
                        </TooltipContent>
                      </Tooltip>
                    ) : null}
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
