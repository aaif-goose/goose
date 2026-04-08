import { SkillCard } from "@site/src/components/skill-card";
import { searchSkills, getAllTags } from "@site/src/utils/skills";
import type { Skill } from "@site/src/pages/skills/types";
import { useState, useEffect } from "react";
import { motion } from "framer-motion";
import Layout from "@theme/Layout";
import Admonition from "@theme/Admonition";
import { Button } from "@site/src/components/ui/button";
import {
  SidebarFilter,
  type SidebarFilterGroup,
} from "@site/src/components/ui/sidebar-filter";
import { Menu, X } from "lucide-react";
import Link from "@docusaurus/Link";

export default function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedFilters, setSelectedFilters] = useState<
    Record<string, string[]>
  >({});
  const [isMobileFilterOpen, setIsMobileFilterOpen] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState(1);
  const skillsPerPage = 10;

  // Build tag filter options from loaded skills
  const uniqueTags = Array.from(new Set(skills.flatMap((s) => s.tags || [])))
    .sort()
    .map((tag) => ({
      label: tag.charAt(0).toUpperCase() + tag.slice(1),
      value: tag,
    }));

  // Build source filter options (Community only - official is the default)
  const sourceOptions = [{ label: "Community", value: "community" }];

  const sidebarFilterGroups: SidebarFilterGroup[] = [
    {
      title: "Source",
      options: sourceOptions,
    },
    {
      title: "Tags",
      options: uniqueTags,
      maxHeight: "max-h-64 overflow-y-auto",
    },
  ];

  useEffect(() => {
    const loadSkills = async () => {
      try {
        setIsLoading(true);
        setError(null);
        const results = await searchSkills(searchQuery);
        setSkills(results);
      } catch (err) {
        const errorMessage =
          err instanceof Error ? err.message : "Unknown error";
        setError(`Failed to load skills: ${errorMessage}`);
        console.error("Error loading skills:", err);
      } finally {
        setIsLoading(false);
      }
    };

    const timeoutId = setTimeout(loadSkills, 300);
    return () => clearTimeout(timeoutId);
  }, [searchQuery]);

  // Apply filters
  let filteredSkills = skills;

  Object.entries(selectedFilters).forEach(([group, values]) => {
    if (values.length > 0) {
      filteredSkills = filteredSkills.filter((skill) => {
        if (group === "Tags") {
          return skill.tags?.some((tag) => values.includes(tag)) ?? false;
        }
        if (group === "Source") {
          // Use isCommunity field from manifest (true if author is not "goose")
          const isCommunity = skill.isCommunity ?? false;
          if (values.includes("community")) return isCommunity;
          return true;
        }
        return true;
      });
    }
  });

  return (
    <Layout
      title="Skills Marketplace"
      description="Browse and install community-contributed skills for goose"
    >
      <div className="container mx-auto px-4 py-8 md:p-24">
        <div className="pb-8 md:pb-16">
          <div className="flex justify-between items-start mb-4">
            <h1 className="text-4xl md:text-[64px] font-medium text-textProminent">
              Skills Marketplace
            </h1>
            <Button
              onClick={() =>
                window.open(
                  "https://github.com/aaif-goose/Agent-Skills?tab=readme-ov-file#contributing-a-skill",
                  "_blank",
                )
              }
              className="bg-purple-600 hover:bg-purple-700 text-white flex items-center gap-2 cursor-pointer"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M12 5v14M5 12h14" />
              </svg>
              Submit Skill
            </Button>
          </div>
          <p className="text-textProminent">
            Browse community-contributed{" "}
            <Link
              to="/docs/guides/context-engineering/using-skills"
              className="text-purple-600 hover:underline"
            >
              skills
            </Link>{" "}
            that teach goose how to perform specific tasks. Skills are reusable
            instruction sets with optional supporting files.
          </p>
        </div>

        <div className="search-container mb-6 md:mb-8">
          <input
            className="bg-bgApp font-light text-textProminent placeholder-textPlaceholder w-full px-3 py-2 md:py-3 text-2xl md:text-[40px] leading-tight md:leading-[52px] border-b border-borderSubtle focus:outline-none focus:ring-purple-500 focus:border-borderProminent caret-[#FF4F00] pl-0"
            placeholder="Search skills by name, description, or tag"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setCurrentPage(1);
            }}
          />
        </div>

        <div className="md:hidden mb-4">
          <Button onClick={() => setIsMobileFilterOpen(!isMobileFilterOpen)}>
            {isMobileFilterOpen ? <X size={20} /> : <Menu size={20} />}
            {isMobileFilterOpen ? "Close Filters" : "Show Filters"}
          </Button>
        </div>

        <div className="flex flex-col md:flex-row gap-8">
          <div
            className={`${isMobileFilterOpen ? "block" : "hidden"} md:block md:w-64 mt-6`}
          >
            <SidebarFilter
              groups={sidebarFilterGroups}
              selectedValues={selectedFilters}
              onChange={(group, values) => {
                setSelectedFilters((prev) => ({ ...prev, [group]: values }));
                setCurrentPage(1);
              }}
            />
          </div>

          <div className="flex-1">
            <div className={`${searchQuery ? "pb-2" : "pb-4 md:pb-8"}`}>
              <p className="text-gray-600">
                {searchQuery
                  ? `${filteredSkills.length} result${filteredSkills.length !== 1 ? "s" : ""} for "${searchQuery}"`
                  : `${filteredSkills.length} skill${filteredSkills.length !== 1 ? "s" : ""} available`}
              </p>
            </div>

            {error && (
              <Admonition type="danger" title="Error">
                <p>{error}</p>
              </Admonition>
            )}

            {isLoading ? (
              <div className="py-8 text-xl text-gray-600">
                Loading skills...
              </div>
            ) : filteredSkills.length === 0 ? (
              <Admonition type="info">
                <p>
                  {searchQuery
                    ? "No skills found matching your search."
                    : "No skills have been submitted yet."}
                </p>
              </Admonition>
            ) : (
              <>
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 md:gap-6">
                  {filteredSkills
                    .slice(
                      (currentPage - 1) * skillsPerPage,
                      currentPage * skillsPerPage,
                    )
                    .map((skill) => (
                      <motion.div
                        key={skill.id}
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={{ duration: 0.6 }}
                      >
                        <SkillCard skill={skill} />
                      </motion.div>
                    ))}
                </div>

                {filteredSkills.length > skillsPerPage && (
                  <div className="flex justify-center items-center gap-2 md:gap-4 mt-6 md:mt-8">
                    <Button
                      onClick={() =>
                        setCurrentPage((prev) => Math.max(prev - 1, 1))
                      }
                      disabled={currentPage === 1}
                      className="px-3 md:px-4 py-2 rounded-md border border-border bg-surfaceHighlight hover:bg-surface text-textProminent disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm md:text-base"
                    >
                      Previous
                    </Button>

                    <span className="text-textProminent text-sm md:text-base">
                      Page {currentPage} of{" "}
                      {Math.ceil(filteredSkills.length / skillsPerPage)}
                    </span>

                    <Button
                      onClick={() =>
                        setCurrentPage((prev) =>
                          Math.min(
                            Math.ceil(filteredSkills.length / skillsPerPage),
                            prev + 1,
                          ),
                        )
                      }
                      disabled={
                        currentPage >=
                        Math.ceil(filteredSkills.length / skillsPerPage)
                      }
                      className="px-3 md:px-4 py-2 rounded-md border border-border bg-surfaceHighlight hover:bg-surface text-textProminent disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm md:text-base"
                    >
                      Next
                    </Button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      </div>
    </Layout>
  );
}
