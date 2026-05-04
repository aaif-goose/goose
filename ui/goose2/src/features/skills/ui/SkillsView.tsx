import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Upload } from "lucide-react";
import { toast } from "sonner";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { Button } from "@/shared/ui/button";
import { PageHeader, PageShell } from "@/shared/ui/page-shell";
import { useSkillImportExport } from "../hooks/useSkillImportExport";
import { SkillDetailPage } from "./SkillDetailPage";
import { SkillsDialogs } from "./SkillsDialogs";
import { SkillsEmptyState } from "./SkillsEmptyState";
import { SkillsListSections } from "./SkillsListSections";
import { SkillsToolbar } from "./SkillsToolbar";
import { hydrateProjectNames } from "../lib/projectHydration";
import {
  filterSkills,
  formatSkillName,
  groupSkills,
  uniqueProjectFilters,
  type SkillsFilter,
} from "../lib/skillsHelpers";
import {
  createSkill,
  deleteSkill,
  listSkills,
  type EditingSkill,
  type SkillInfo,
} from "../api/skills";
import {
  uniqueSkillCategories,
  withInferredSkillCategories,
  type SkillCategory,
  type SkillViewInfo,
} from "../lib/skillCategories";

function getDuplicateSkillName(name: string, existingNames: Set<string>) {
  const baseName = formatSkillName(`${name}-copy`) || "skill-copy";
  if (!existingNames.has(baseName)) {
    return baseName;
  }

  for (let index = 2; index < 1000; index += 1) {
    const suffix = `-${index}`;
    const prefix =
      baseName.slice(0, 64 - suffix.length).replace(/-+$/g, "") || "skill";
    const candidate = `${prefix}${suffix}`;
    if (!existingNames.has(candidate)) {
      return candidate;
    }
  }

  return `skill-copy-${Date.now().toString().slice(-8)}`;
}

interface SkillsViewProps {
  onStartChatWithSkill?: (skill: SkillInfo, projectId?: string | null) => void;
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
  const [editingSkill, setEditingSkill] = useState<EditingSkill | undefined>(
    undefined,
  );
  const [skills, setSkills] = useState<SkillViewInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [deletingSkill, setDeletingSkill] = useState<SkillInfo | null>(null);
  const [activeSkillId, setActiveSkillId] = useState<string | null>(null);
  const [expandedSectionIds, setExpandedSectionIds] = useState<string[]>([]);
  const loadRequestIdRef = useRef(0);

  const loadSkills = useCallback(async (): Promise<SkillViewInfo[]> => {
    const requestId = loadRequestIdRef.current + 1;
    loadRequestIdRef.current = requestId;
    setLoading(true);

    try {
      const projectDirs = projects.flatMap((project) => project.workingDirs);
      const result = await listSkills(projectDirs);
      if (loadRequestIdRef.current !== requestId) {
        return [];
      }
      const nextSkills = withInferredSkillCategories(
        hydrateProjectNames(result, projects),
      );
      setSkills(nextSkills);
      return nextSkills;
    } catch {
      if (loadRequestIdRef.current === requestId) {
        toast.error(t("view.loadError"));
      }
      return [];
    } finally {
      if (loadRequestIdRef.current === requestId) {
        setLoading(false);
      }
    }
  }, [projects, t]);

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

  const filteredSkills = useMemo(
    () =>
      filterSkills(
        skills,
        { search, activeFilter, selectedCategories },
        (category) => t(`view.categories.options.${category}`),
      ),
    [skills, search, activeFilter, selectedCategories, t],
  );

  const groupedSkills = useMemo(
    () =>
      groupSkills(filteredSkills, activeFilter, projectFilters, {
        personalTitle: t("view.filtersGlobal"),
        projectsFallback: t("view.projects"),
      }),
    [filteredSkills, activeFilter, projectFilters, t],
  );

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
      toast.success(t("view.deleteSuccess", { name: deletingSkill.name }));
    } catch {
      toast.error(t("view.deleteError"));
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

  const handleDuplicate = useCallback(
    async (skill: SkillInfo) => {
      const duplicateName = getDuplicateSkillName(
        skill.name,
        new Set(skills.map((currentSkill) => currentSkill.name)),
      );

      try {
        await createSkill(duplicateName, skill.description, skill.instructions);
        await loadSkills();
        toast.success(t("view.duplicated", { name: duplicateName }));
      } catch {
        toast.error(t("view.duplicateError"));
      }
    },
    [loadSkills, skills, t],
  );

  const handleStartChat = useCallback(
    (skill: SkillInfo) => {
      onStartChatWithSkill?.(skill, skill.projectLinks[0]?.id ?? null);
    },
    [onStartChatWithSkill],
  );

  const handleDialogClose = () => {
    setDialogOpen(false);
    setEditingSkill(undefined);
  };

  const handleNewSkill = () => {
    setEditingSkill(undefined);
    setDialogOpen(true);
  };

  const handleSkillSaved = useCallback(
    async (savedSkill?: SkillInfo) => {
      const refreshedSkills = await loadSkills();
      if (
        savedSkill &&
        refreshedSkills.some((skill) => skill.id === savedSkill.id)
      ) {
        setActiveSkillId(savedSkill.id);
      }
    },
    [loadSkills],
  );

  const refreshSkills = useCallback(async () => {
    await loadSkills();
  }, [loadSkills]);

  const {
    fileInputRef,
    isDragOver,
    dropHandlers,
    handleFileChange,
    openFilePicker,
    handleCopyFile,
    handleSaveCopy,
  } = useSkillImportExport(refreshSkills);

  const handleSelectSkill = (skill: SkillViewInfo) => {
    setActiveSkillId(skill.id);
  };

  const dialogs = (
    <SkillsDialogs
      dialogOpen={dialogOpen}
      onDialogClose={handleDialogClose}
      onSaved={handleSkillSaved}
      editingSkill={editingSkill}
      deletingSkill={deletingSkill}
      onDeletingSkillChange={setDeletingSkill}
      onConfirmDelete={handleConfirmDeleteSkill}
    />
  );

  if (activeSkill) {
    return (
      <>
        <SkillDetailPage
          skill={activeSkill}
          onBack={() => setActiveSkillId(null)}
          onEdit={handleEdit}
          onCopyFile={handleCopyFile}
          onSaveCopy={handleSaveCopy}
          onStartChat={onStartChatWithSkill ? handleStartChat : undefined}
          onDuplicate={handleDuplicate}
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
            <Button
              type="button"
              variant="outline-flat"
              size="xs"
              onClick={openFilePicker}
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

      <SkillsToolbar
        search={search}
        onSearchChange={setSearch}
        activeFilter={activeFilter}
        onActiveFilterChange={setActiveFilter}
        projectFilters={projectFilters}
        categoryFilters={categoryFilters}
        selectedCategories={selectedCategories}
        onSelectedCategoriesChange={setSelectedCategories}
        dropHandlers={dropHandlers}
        isDragOver={isDragOver}
      />

      {filteredSkills.length > 0 ? (
        <SkillsListSections
          sections={groupedSkills}
          expandedSectionIds={expandedSectionIds}
          onExpandedSectionIdsChange={setExpandedSectionIds}
          onSelectSkill={handleSelectSkill}
          onStartChat={onStartChatWithSkill ? handleStartChat : undefined}
          onEdit={handleEdit}
          onCopyFile={handleCopyFile}
          onSaveCopy={handleSaveCopy}
          onDuplicate={handleDuplicate}
          onDelete={handleDelete}
        />
      ) : null}

      {!loading && filteredSkills.length === 0 ? (
        <SkillsEmptyState
          hasAnySkills={skills.length > 0}
          isDragOver={isDragOver}
          dropHandlers={dropHandlers}
          onNewSkill={handleNewSkill}
          onImport={openFilePicker}
        />
      ) : null}

      <input
        ref={fileInputRef}
        type="file"
        accept=".md,text/markdown,text/plain"
        className="hidden"
        onChange={handleFileChange}
      />

      {dialogs}
    </PageShell>
  );
}
