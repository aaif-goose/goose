import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { SearchBar } from "@/shared/ui/SearchBar";
import { Button } from "@/shared/ui/button";
import { FilterRow } from "@/shared/ui/page-shell";
import type { SkillsFilter } from "../lib/skillsHelpers";

interface SkillsToolbarProps {
  search: string;
  onSearchChange: (value: string) => void;
  activeFilter: SkillsFilter;
  onActiveFilterChange: (filter: SkillsFilter) => void;
  hasBuiltinSkills: boolean;
  projectFilters: { id: string; name: string }[];
  dropHandlers?: React.HTMLAttributes<HTMLDivElement>;
  isDragOver?: boolean;
}

function FilterButton({
  active,
  children,
  onClick,
}: {
  active: boolean;
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <Button
      type="button"
      size="xs"
      variant={active ? "default" : "outline-flat"}
      onClick={onClick}
    >
      {children}
    </Button>
  );
}

export function SkillsToolbar({
  search,
  onSearchChange,
  activeFilter,
  onActiveFilterChange,
  hasBuiltinSkills,
  projectFilters,
  dropHandlers,
  isDragOver,
}: SkillsToolbarProps) {
  const { t } = useTranslation(["skills"]);

  return (
    <div
      {...dropHandlers}
      className={cn(
        "space-y-3 rounded-2xl transition-colors",
        isDragOver && "bg-muted/50",
      )}
    >
      <SearchBar
        value={search}
        onChange={onSearchChange}
        placeholder={t("view.searchPlaceholder")}
      />

      <FilterRow>
        <FilterButton
          active={activeFilter === "all"}
          onClick={() => onActiveFilterChange("all")}
        >
          {t("view.filtersAllSources")}
        </FilterButton>
        <FilterButton
          active={activeFilter === "global"}
          onClick={() => onActiveFilterChange("global")}
        >
          {t("view.filtersGlobal")}
        </FilterButton>
        {hasBuiltinSkills ? (
          <FilterButton
            active={activeFilter === "builtin"}
            onClick={() => onActiveFilterChange("builtin")}
          >
            {t("view.filtersBuiltin")}
          </FilterButton>
        ) : null}
        {projectFilters.map((project) => {
          const filterValue = `project:${project.id}` as const;
          return (
            <FilterButton
              key={project.id}
              active={activeFilter === filterValue}
              onClick={() => onActiveFilterChange(filterValue)}
            >
              {project.name}
            </FilterButton>
          );
        })}
      </FilterRow>
    </div>
  );
}
