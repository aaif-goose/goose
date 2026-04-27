import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Upload } from "lucide-react";
import {
  IconAdjustmentsHorizontal,
  IconChevronDown,
} from "@tabler/icons-react";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { cn } from "@/shared/lib/cn";
import { SearchBar } from "@/shared/ui/SearchBar";
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
import { FilterRow, PageHeader, PageShell } from "@/shared/ui/page-shell";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";
import { revealInFileManager } from "@/shared/lib/fileManager";
import { SkillDetailPage } from "./SkillDetailPage";
import { SkillsDialogs } from "./SkillsDialogs";
import { SkillsEmptyState } from "./SkillsEmptyState";
import { SkillsListSections, type SkillsSection } from "./SkillsListSections";
import { hydrateProjectNames } from "../lib/projectHydration";
import {
  compareSkillsByName,
  downloadExport,
  uniqueSkillCategories,
  uniqueProjectFilters,
} from "../lib/skillsHelpers";
import {
  deleteSkill,
  exportSkill,
  importSkills,
  listSkills,
  type SkillInfo,
} from "../api/skills";
import {
  withInferredSkillCategories,
  type SkillCategory,
  type SkillViewInfo,
} from "../lib/skillCategories";

type SkillsFilter = "all" | "global" | `project:${string}`;

function SkillCategoryFilter({
  categories,
  selectedCategories,
  onSelectedCategoriesChange,
}: {
  categories: SkillCategory[];
  selectedCategories: SkillCategory[];
  onSelectedCategoriesChange: (categories: SkillCategory[]) => void;
}) {
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

interface SkillsViewProps {
  onStartChatWithSkill?: (skillName: string, projectId?: string | null) => void;
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

export function SkillsView({ onStartChatWithSkill }: SkillsViewProps) {
  const { t } = useTranslation(["skills", "common"]);
  const projects = useProjectStore((state) => state.projects);
  const [search, setSearch] = useState("");
  const [activeFilter, setActiveFilter] = useState<SkillsFilter>("all");
  const [selectedCategories, setSelectedCategories] = useState<SkillCategory[]>(
    [],
  );
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingSkill, setEditingSkill] = useState<
    | {
        name: string;
        description: string;
        instructions: string;
        path: string;
        fileLocation: string;
      }
    | undefined
  >(undefined);
  const [skills, setSkills] = useState<SkillViewInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [deletingSkill, setDeletingSkill] = useState<SkillInfo | null>(null);
  const [notification, setNotification] = useState<string | null>(null);
  const [activeSkillId, setActiveSkillId] = useState<string | null>(null);
  const [expandedSectionIds, setExpandedSectionIds] = useState<string[]>([]);
  const importInputRef = useRef<HTMLInputElement>(null);
  const loadRequestIdRef = useRef(0);

  const loadSkills = useCallback(async () => {
    const requestId = loadRequestIdRef.current + 1;
    loadRequestIdRef.current = requestId;
    setLoading(true);

    try {
      const projectDirs = projects.flatMap((project) => project.workingDirs);
      const result = await listSkills(projectDirs);
      if (loadRequestIdRef.current !== requestId) {
        return;
      }
      setSkills(
        withInferredSkillCategories(hydrateProjectNames(result, projects)),
      );
    } catch {
      if (loadRequestIdRef.current === requestId) {
        setSkills([]);
      }
    } finally {
      if (loadRequestIdRef.current === requestId) {
        setLoading(false);
      }
    }
  }, [projects]);

  useEffect(() => {
    loadSkills();
  }, [loadSkills]);

  const projectFilters = useMemo(() => uniqueProjectFilters(skills), [skills]);
  const categoryFilters = useMemo(
    () => uniqueSkillCategories(skills),
    [skills],
  );

  useEffect(() => {
    if (!activeFilter.startsWith("project:")) {
      return;
    }

    const projectId = activeFilter.slice("project:".length);
    if (!projectFilters.some((project) => project.id === projectId)) {
      setActiveFilter("all");
    }
  }, [activeFilter, projectFilters]);

  useEffect(() => {
    setSelectedCategories((current) =>
      current.filter((category) => categoryFilters.includes(category)),
    );
  }, [categoryFilters]);

  const filteredSkills = useMemo(() => {
    const searchTerm = search.trim().toLowerCase();
    return skills.filter((skill) => {
      const matchesSearch =
        searchTerm.length === 0 ||
        skill.name.toLowerCase().includes(searchTerm) ||
        skill.description.toLowerCase().includes(searchTerm) ||
        skill.sourceLabel.toLowerCase().includes(searchTerm) ||
        t(`view.categories.options.${skill.inferredCategory}`)
          .toLowerCase()
          .includes(searchTerm);

      const matchesFilter =
        activeFilter === "all"
          ? true
          : activeFilter === "global"
            ? skill.sourceKind === "global"
            : skill.projectLinks.some(
                (project) => `project:${project.id}` === activeFilter,
              );

      const matchesCategory =
        selectedCategories.length === 0 ||
        selectedCategories.includes(skill.inferredCategory);

      return matchesSearch && matchesFilter && matchesCategory;
    });
  }, [activeFilter, search, selectedCategories, skills, t]);

  const groupedSkills = useMemo<SkillsSection[]>(() => {
    if (activeFilter === "global") {
      return [
        {
          id: "personal",
          title: t("view.filtersGlobal"),
          skills: [...filteredSkills].sort(compareSkillsByName),
        },
      ];
    }

    if (activeFilter.startsWith("project:")) {
      const projectId = activeFilter.slice("project:".length);
      const projectName =
        projectFilters.find((project) => project.id === projectId)?.name ??
        t("view.projects");

      return [
        {
          id: activeFilter,
          title: projectName,
          skills: [...filteredSkills].sort(compareSkillsByName),
        },
      ];
    }

    const personalSkills = filteredSkills
      .filter((skill) => skill.sourceKind === "global")
      .sort(compareSkillsByName);

    const projectSections = projectFilters
      .map((project) => ({
        id: `project:${project.id}`,
        title: project.name,
        skills: filteredSkills
          .filter((skill) =>
            skill.projectLinks.some((link) => link.id === project.id),
          )
          .sort(compareSkillsByName),
      }))
      .filter((section) => section.skills.length > 0);

    return [
      ...(personalSkills.length > 0
        ? [
            {
              id: "personal",
              title: t("view.filtersGlobal"),
              skills: personalSkills,
            },
          ]
        : []),
      ...projectSections,
    ];
  }, [activeFilter, filteredSkills, projectFilters, t]);

  useEffect(() => {
    const nextIds = groupedSkills.map((section) => section.id);
    setExpandedSectionIds((prev) => {
      const stillVisible = prev.filter((id) => nextIds.includes(id));
      const newIds = nextIds.filter((id) => !stillVisible.includes(id));
      return [...stillVisible, ...newIds];
    });
  }, [groupedSkills]);

  const activeSkill =
    skills.find((skill) => skill.id === activeSkillId) ?? null;

  const handleDelete = (skill: SkillInfo) => {
    setDeletingSkill(skill);
  };

  const handleConfirmDeleteSkill = async () => {
    if (!deletingSkill) return;
    try {
      await deleteSkill(deletingSkill.path);
      await loadSkills();
      if (activeSkillId === deletingSkill.id) {
        setActiveSkillId(null);
      }
    } catch {
      // best-effort
    }
    setDeletingSkill(null);
  };

  const handleEdit = (skill: SkillInfo) => {
    setEditingSkill({
      name: skill.name,
      description: skill.description,
      instructions: skill.instructions,
      path: skill.path,
      fileLocation: skill.fileLocation,
    });
    setDialogOpen(true);
  };

  const handleExport = async (skill: SkillInfo) => {
    try {
      const result = await exportSkill(skill.path);
      downloadExport(result.json, result.filename);
      setNotification(t("view.exportedTo", { filename: result.filename }));
      setTimeout(() => setNotification(null), 3000);
    } catch (err) {
      console.error("Failed to export skill:", err);
    }
  };

  const handleReveal = useCallback((skill: SkillInfo) => {
    void revealInFileManager(skill.path);
  }, []);

  const handleStartChat = useCallback(
    (skill: SkillInfo) => {
      onStartChatWithSkill?.(skill.name, skill.projectLinks[0]?.id ?? null);
    },
    [onStartChatWithSkill],
  );

  const handleImportFile = useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      if (!file) return;

      try {
        const arrayBuffer = await file.arrayBuffer();
        const bytes = Array.from(new Uint8Array(arrayBuffer));
        await importSkills(bytes, file.name);
        await loadSkills();
      } catch (error) {
        console.error("Failed to import skill:", error);
      }

      if (importInputRef.current) {
        importInputRef.current.value = "";
      }
    },
    [loadSkills],
  );

  const handleDialogClose = () => {
    setDialogOpen(false);
    setEditingSkill(undefined);
  };

  const handleNewSkill = () => {
    setEditingSkill(undefined);
    setDialogOpen(true);
  };

  const handleDropImport = useCallback(
    async (fileBytes: number[], fileName: string) => {
      try {
        await importSkills(fileBytes, fileName);
        await loadSkills();
      } catch (error) {
        console.error("Failed to import skill:", error);
      }
    },
    [loadSkills],
  );

  const {
    fileInputRef: dropFileInputRef,
    isDragOver,
    dropHandlers,
    handleFileChange: handleDropFileChange,
  } = useFileImportZone({ onImportFile: handleDropImport });

  const handleSelectSkill = (skill: SkillViewInfo) => {
    setActiveSkillId(skill.id);
  };

  const dialogs = (
    <SkillsDialogs
      dialogOpen={dialogOpen}
      onDialogClose={handleDialogClose}
      onCreated={loadSkills}
      editingSkill={editingSkill}
      deletingSkill={deletingSkill}
      onDeletingSkillChange={setDeletingSkill}
      onConfirmDelete={handleConfirmDeleteSkill}
      notification={notification}
    />
  );

  if (activeSkill) {
    return (
      <>
        <SkillDetailPage
          skill={activeSkill}
          onBack={() => setActiveSkillId(null)}
          onEdit={handleEdit}
          onReveal={handleReveal}
          onShare={handleExport}
          onStartChat={onStartChatWithSkill ? handleStartChat : undefined}
          onDelete={handleDelete}
        />
        {dialogs}
      </>
    );
  }

  return (
    <PageShell>
      <PageHeader
        title={t("view.title")}
        description={t("view.description")}
        titleClassName="font-normal text-foreground"
        actions={
          <>
            <input
              ref={importInputRef}
              type="file"
              accept=".skill.json,.json"
              className="hidden"
              onChange={handleImportFile}
            />
            <Button
              type="button"
              variant="outline-flat"
              size="xs"
              onClick={() => importInputRef.current?.click()}
            >
              <Upload className="size-3.5" />
              {t("common:actions.import")}
            </Button>
            <Button
              type="button"
              variant="outline-flat"
              size="xs"
              onClick={handleNewSkill}
            >
              <Plus className="size-3.5" />
              {t("view.newSkill")}
            </Button>
          </>
        }
      />

      <div
        {...dropHandlers}
        className={cn(
          "rounded-2xl transition-colors",
          isDragOver && "bg-muted/50",
        )}
      >
        <div className="space-y-3">
          <SearchBar
            value={search}
            onChange={setSearch}
            placeholder={t("view.searchPlaceholder")}
          />

          <FilterRow>
            <FilterButton
              active={activeFilter === "all"}
              onClick={() => setActiveFilter("all")}
            >
              {t("view.filtersAllSources")}
            </FilterButton>
            <FilterButton
              active={activeFilter === "global"}
              onClick={() => setActiveFilter("global")}
            >
              {t("view.filtersGlobal")}
            </FilterButton>
            {projectFilters.map((project) => {
              const filterValue = `project:${project.id}` as const;
              return (
                <FilterButton
                  key={project.id}
                  active={activeFilter === filterValue}
                  onClick={() => setActiveFilter(filterValue)}
                >
                  {project.name}
                </FilterButton>
              );
            })}
            {categoryFilters.length > 0 ? (
              <SkillCategoryFilter
                categories={categoryFilters}
                selectedCategories={selectedCategories}
                onSelectedCategoriesChange={setSelectedCategories}
              />
            ) : null}
          </FilterRow>
        </div>
      </div>

      {!loading && filteredSkills.length > 0 ? (
        <SkillsListSections
          sections={groupedSkills}
          expandedSectionIds={expandedSectionIds}
          onExpandedSectionIdsChange={setExpandedSectionIds}
          onSelectSkill={handleSelectSkill}
          onStartChat={onStartChatWithSkill ? handleStartChat : undefined}
        />
      ) : null}

      {!loading && filteredSkills.length === 0 ? (
        <SkillsEmptyState
          hasAnySkills={skills.length > 0}
          isDragOver={isDragOver}
          dropHandlers={dropHandlers}
          onNewSkill={handleNewSkill}
          onImport={() => importInputRef.current?.click()}
        />
      ) : null}

      <input
        ref={dropFileInputRef}
        type="file"
        accept=".skill.json,.json"
        className="hidden"
        onChange={handleDropFileChange}
      />

      {dialogs}
    </PageShell>
  );
}
