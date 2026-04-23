import { useTranslation } from "react-i18next";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionSectionTrigger,
} from "@/shared/ui/accordion";
import { Button } from "@/shared/ui/button";
import { formatSkillCount } from "../lib/skillsHelpers";
import type { SkillInfo } from "../api/skills";

export interface SkillsSection {
  id: string;
  title: string;
  skills: SkillInfo[];
}

interface SkillsListSectionsProps {
  sections: SkillsSection[];
  expandedSectionIds: string[];
  onExpandedSectionIdsChange: (ids: string[]) => void;
  onSelectSkill: (skill: SkillInfo) => void;
  onStartChat: (skill: SkillInfo) => void;
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
            meta={formatSkillCount(section.skills.length)}
          />

          <AccordionContent className="pb-0">
            <div className="motion-safe:group-data-[state=closed]/skills-section:animate-accordion-content-close motion-safe:group-data-[state=open]/skills-section:animate-accordion-content-open border-t border-border-soft-divider will-change-[opacity,transform]">
              <div className="divide-y divide-border-soft-divider">
                {section.skills.map((skill) => (
                  <div
                    key={`${section.id}-${skill.id}`}
                    className="group px-5 py-4 transition-colors hover:bg-muted/20"
                  >
                    <button
                      type="button"
                      className="block w-full min-w-0 text-left"
                      onClick={() => onSelectSkill(skill)}
                      aria-label={t("view.openDetails", { name: skill.name })}
                    >
                      <div className="min-w-0">
                        <div className="flex items-center gap-2">
                          <p className="text-sm font-normal text-foreground">
                            {skill.name}
                          </p>
                          <Button
                            type="button"
                            variant="inline-subtle"
                            size="xs"
                            className="opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
                            onClick={(event) => {
                              event.stopPropagation();
                              onStartChat(skill);
                            }}
                            aria-label={t("view.startChat", {
                              name: skill.name,
                            })}
                          >
                            {t("view.useInChat")}
                          </Button>
                        </div>
                        {skill.description ? (
                          <p className="mt-1 line-clamp-2 text-xs font-light text-muted-foreground">
                            {skill.description}
                          </p>
                        ) : null}
                      </div>
                    </button>
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
