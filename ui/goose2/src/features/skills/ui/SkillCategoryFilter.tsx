import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  IconAdjustmentsHorizontal,
  IconChevronDown,
} from "@tabler/icons-react";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import type { SkillCategory } from "../lib/skillCategories";

interface SkillCategoryFilterProps {
  categories: SkillCategory[];
  selectedCategories: SkillCategory[];
  onSelectedCategoriesChange: (categories: SkillCategory[]) => void;
}

export function SkillCategoryFilter({
  categories,
  selectedCategories,
  onSelectedCategoriesChange,
}: SkillCategoryFilterProps) {
  const { t } = useTranslation(["skills"]);

  const toggleCategory = useCallback(
    (category: SkillCategory) => {
      onSelectedCategoriesChange(
        selectedCategories.includes(category)
          ? selectedCategories.filter((value) => value !== category)
          : [...selectedCategories, category],
      );
    },
    [onSelectedCategoriesChange, selectedCategories],
  );

  const buttonLabel =
    selectedCategories.length === 0
      ? t("view.categories.label")
      : selectedCategories.length === 1
        ? t(`view.categories.options.${selectedCategories[0]}`)
        : t("view.categories.count", { count: selectedCategories.length });

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          size="xs"
          variant={selectedCategories.length > 0 ? "default" : "outline-flat"}
          leftIcon={<IconAdjustmentsHorizontal />}
          rightIcon={<IconChevronDown />}
          aria-label={t("view.categories.filter")}
        >
          {buttonLabel}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuLabel>{t("view.categories.label")}</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {categories.map((category) => (
          <DropdownMenuCheckboxItem
            key={category}
            checked={selectedCategories.includes(category)}
            onSelect={(event) => event.preventDefault()}
            onCheckedChange={() => toggleCategory(category)}
          >
            {t(`view.categories.options.${category}`)}
          </DropdownMenuCheckboxItem>
        ))}
        {selectedCategories.length > 0 ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onSelect={(event) => {
                event.preventDefault();
                onSelectedCategoriesChange([]);
              }}
            >
              {t("view.categories.clear")}
            </DropdownMenuItem>
          </>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
