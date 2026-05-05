import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconChevronDown, IconPlus } from "@tabler/icons-react";
import { Button } from "@/shared/ui/button";
import { SearchBar } from "@/shared/ui/SearchBar";
import { FilterRow, PageHeader } from "@/shared/ui/page-shell";
import { useExtensionsSettings } from "../hooks/useExtensionsSettings";
import {
  EXTENSION_CATEGORIES,
  filterExtensions,
  getExtensionCategoryCounts,
  splitExtensionsByCategory,
  type ExtensionFilter,
} from "../lib/extensionCategories";
import type { ExtensionEntry } from "../types";
import { ExtensionItem } from "./ExtensionItem";
import { ExtensionModal } from "./ExtensionModal";

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

export function ExtensionsSettings() {
  const { t } = useTranslation("settings");
  const {
    extensions,
    isLoading,
    modalMode,
    editingExtension,
    handleAdd,
    handleConfigure,
    handleSubmit,
    handleDelete,
    handleModalClose,
  } = useExtensionsSettings();
  const [searchTerm, setSearchTerm] = useState("");
  const [activeFilter, setActiveFilter] = useState<ExtensionFilter>("all");
  const [showGooseCapabilities, setShowGooseCapabilities] = useState(false);

  const filteredExtensions = useMemo(
    () =>
      filterExtensions({
        extensions,
        searchTerm,
        activeFilter,
        getCategoryLabel: (category) => t(`extensions.categories.${category}`),
      }),
    [activeFilter, extensions, searchTerm, t],
  );

  const { primaryExtensions, gooseCapabilities } = useMemo(
    () => splitExtensionsByCategory(filteredExtensions),
    [filteredExtensions],
  );

  const visibleExtensions =
    activeFilter === "gooseCapabilities"
      ? gooseCapabilities
      : [...primaryExtensions, ...gooseCapabilities];
  const hasSearch = searchTerm.trim().length > 0;
  const shouldShowGooseCapabilities =
    activeFilter === "gooseCapabilities" || showGooseCapabilities || hasSearch;
  const showGooseCapabilitiesToggle =
    activeFilter !== "gooseCapabilities" &&
    !hasSearch &&
    gooseCapabilities.length > 0;

  const categoryCounts = useMemo(
    () => getExtensionCategoryCounts(extensions),
    [extensions],
  );

  const renderSection = (
    title: string,
    sectionExtensions: ExtensionEntry[],
    showTitle = true,
  ) => {
    if (sectionExtensions.length === 0) return null;
    return (
      <section className="space-y-3">
        {showTitle ? (
          <h4 className="text-sm font-normal text-foreground">{title}</h4>
        ) : null}
        <div className="grid gap-x-12 sm:grid-cols-2">
          {sectionExtensions.map((ext) => (
            <ExtensionItem
              key={ext.config_key}
              extension={ext}
              onConfigure={handleConfigure}
            />
          ))}
        </div>
      </section>
    );
  };

  return (
    <>
      <PageHeader
        title={t("extensions.title")}
        description={t("extensions.description")}
        titleClassName="font-normal text-foreground"
        actions={
          <Button
            type="button"
            variant="outline-flat"
            size="xs"
            onClick={handleAdd}
          >
            <IconPlus className="size-3.5" />
            {t("extensions.addExtension")}
          </Button>
        }
      />

      <div className="space-y-3">
        <SearchBar
          value={searchTerm}
          onChange={setSearchTerm}
          placeholder={t("extensions.search")}
          aria-label={t("extensions.search")}
        />
        <FilterRow>
          <FilterButton
            active={activeFilter === "all"}
            onClick={() => setActiveFilter("all")}
          >
            {t("extensions.filters.all")}
          </FilterButton>
          {EXTENSION_CATEGORIES.map((category) =>
            categoryCounts[category] > 0 ? (
              <FilterButton
                key={category}
                active={activeFilter === category}
                onClick={() => setActiveFilter(category)}
              >
                {t(`extensions.categories.${category}`)}
              </FilterButton>
            ) : null,
          )}
        </FilterRow>
      </div>

      {isLoading ? (
        <div className="grid gap-x-12 sm:grid-cols-2">
          {[1, 2, 3, 4, 5, 6].map((i) => (
            <div
              key={i}
              className="h-20 animate-pulse border-b border-border-soft-divider py-4"
            >
              <div className="h-4 w-2/5 rounded bg-muted/50" />
              <div className="mt-2 h-3 w-3/5 rounded bg-muted/40" />
            </div>
          ))}
        </div>
      ) : extensions.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("extensions.empty")}</p>
      ) : visibleExtensions.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t("extensions.noResults")}
        </p>
      ) : (
        <div className="space-y-8">
          {activeFilter !== "gooseCapabilities"
            ? renderSection(
                t("extensions.sections.extensions"),
                primaryExtensions,
                false,
              )
            : null}

          {shouldShowGooseCapabilities
            ? renderSection(
                t("extensions.sections.gooseCapabilities"),
                gooseCapabilities,
              )
            : null}

          {showGooseCapabilitiesToggle ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setShowGooseCapabilities((current) => !current)}
              className="w-full text-muted-foreground"
            >
              {showGooseCapabilities
                ? t("extensions.hideGooseCapabilities")
                : t("extensions.showGooseCapabilities", {
                    count: gooseCapabilities.length,
                  })}
              {!showGooseCapabilities ? (
                <IconChevronDown className="size-3" />
              ) : null}
            </Button>
          ) : null}
        </div>
      )}

      {modalMode === "add" && (
        <ExtensionModal onSubmit={handleSubmit} onClose={handleModalClose} />
      )}

      {modalMode === "edit" && editingExtension && (
        <ExtensionModal
          extension={editingExtension}
          onSubmit={handleSubmit}
          onDelete={handleDelete}
          onClose={handleModalClose}
        />
      )}
    </>
  );
}
