import { useCallback, useMemo, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { isReservedSlashCommand } from "@/features/skills/lib/skillChatPrompt";
import type { Persona } from "@/shared/types/agents";

export function fuzzyMatch(query: string, target: string): boolean {
  let qi = 0;
  for (let ti = 0; ti < target.length && qi < query.length; ti++) {
    if (query[qi] === target[ti]) qi++;
  }
  return qi === query.length;
}

export interface FileMentionItem {
  resolvedPath: string;
  displayPath: string;
  filename: string;
  kind: "file" | "folder" | "path";
}

export interface SkillMentionItem {
  id: string;
  name: string;
  description: string;
  sourceLabel: string;
}

export type MentionItem =
  | { type: "persona"; persona: Persona }
  | { type: "skill"; skill: SkillMentionItem }
  | { type: "file"; file: FileMentionItem };

export function useMentionDetection(
  personas: Persona[] = [],
  skills: SkillMentionItem[] = [],
  files: FileMentionItem[] = [],
) {
  const [mentionState, setMentionState] = useState<{
    isOpen: boolean;
    trigger: "@" | "/";
    query: string;
    startIndex: number;
    selectedIndex: number;
  }>({
    isOpen: false,
    trigger: "@",
    query: "",
    startIndex: -1,
    selectedIndex: 0,
  });

  const { filteredPersonas, filteredSkills, filteredFiles } = useMemo(() => {
    if (!mentionState.isOpen) {
      return {
        filteredPersonas: personas,
        filteredSkills: skills,
        filteredFiles: files,
      };
    }

    const q = mentionState.query.toLowerCase();
    const matchesSkill = (skill: SkillMentionItem) =>
      fuzzyMatch(q, skill.name.toLowerCase()) ||
      fuzzyMatch(q, skill.description.toLowerCase()) ||
      fuzzyMatch(q, skill.sourceLabel.toLowerCase());
    const matchingSkills = q ? skills.filter(matchesSkill) : skills;

    if (mentionState.trigger === "/") {
      return {
        filteredPersonas: [],
        filteredSkills: matchingSkills,
        filteredFiles: [],
      };
    }

    if (!q) {
      return {
        filteredPersonas: personas,
        filteredSkills: skills,
        filteredFiles: files,
      };
    }

    return {
      filteredPersonas: personas.filter((p) =>
        fuzzyMatch(q, p.displayName.toLowerCase()),
      ),
      filteredSkills: matchingSkills,
      filteredFiles: files.filter(
        (f) =>
          fuzzyMatch(q, f.filename.toLowerCase()) ||
          fuzzyMatch(q, f.displayPath.toLowerCase()),
      ),
    };
  }, [
    personas,
    skills,
    files,
    mentionState.isOpen,
    mentionState.query,
    mentionState.trigger,
  ]);

  const totalCount =
    filteredPersonas.length + filteredSkills.length + filteredFiles.length;

  const detectMention = useCallback(
    (value: string, cursorPos: number) => {
      const beforeCursor = value.slice(0, cursorPos);
      const lastAt = beforeCursor.lastIndexOf("@");
      const slashAtStart = beforeCursor.startsWith("/") ? 0 : -1;

      if (lastAt === -1 && slashAtStart === -1) {
        if (mentionState.isOpen) closeMentionState(setMentionState);
        return;
      }

      if (slashAtStart === 0 && lastAt === -1) {
        const query = beforeCursor.slice(1);
        if (
          query.includes(" ") ||
          query.length > 50 ||
          isReservedSlashCommand(query)
        ) {
          if (mentionState.isOpen) closeMentionState(setMentionState);
          return;
        }

        setMentionState((prev) => ({
          isOpen: true,
          trigger: "/",
          query,
          startIndex: 0,
          selectedIndex:
            prev.query !== query || prev.trigger !== "/"
              ? 0
              : prev.selectedIndex,
        }));
        return;
      }

      if (lastAt > 0 && !/\s/.test(beforeCursor[lastAt - 1])) {
        if (mentionState.isOpen) closeMentionState(setMentionState);
        return;
      }

      const query = beforeCursor.slice(lastAt + 1);
      if (query.includes(" ") || query.length > 50) {
        if (mentionState.isOpen) closeMentionState(setMentionState);
        return;
      }

      setMentionState((prev) => ({
        isOpen: true,
        trigger: "@",
        query,
        startIndex: lastAt,
        selectedIndex:
          prev.query !== query || prev.trigger !== "@" ? 0 : prev.selectedIndex,
      }));
    },
    [mentionState.isOpen],
  );

  const closeMention = useCallback(() => {
    closeMentionState(setMentionState);
  }, []);

  const navigateMention = useCallback(
    (direction: "up" | "down"): boolean => {
      if (!mentionState.isOpen || totalCount === 0) return false;
      setMentionState((prev) => {
        const delta = direction === "down" ? 1 : -1;
        const next = (prev.selectedIndex + delta + totalCount) % totalCount;
        return { ...prev, selectedIndex: next };
      });
      return true;
    },
    [mentionState.isOpen, totalCount],
  );

  const confirmMention = useCallback((): MentionItem | null => {
    if (!mentionState.isOpen || totalCount === 0) return null;
    const idx = mentionState.selectedIndex;
    if (idx < filteredPersonas.length) {
      return { type: "persona", persona: filteredPersonas[idx] };
    }
    const skillIdx = idx - filteredPersonas.length;
    if (skillIdx < filteredSkills.length) {
      return { type: "skill", skill: filteredSkills[skillIdx] };
    }
    const fileIdx = skillIdx - filteredSkills.length;
    if (fileIdx < filteredFiles.length) {
      return { type: "file", file: filteredFiles[fileIdx] };
    }
    return null;
  }, [
    mentionState.isOpen,
    mentionState.selectedIndex,
    totalCount,
    filteredPersonas,
    filteredSkills,
    filteredFiles,
  ]);

  return {
    mentionOpen: mentionState.isOpen,
    mentionQuery: mentionState.query,
    mentionStartIndex: mentionState.startIndex,
    mentionSelectedIndex: mentionState.selectedIndex,
    filteredPersonas,
    filteredSkills,
    filteredFiles,
    detectMention,
    closeMention,
    navigateMention,
    confirmMention,
  };
}

function closeMentionState(
  setMentionState: Dispatch<
    SetStateAction<{
      isOpen: boolean;
      trigger: "@" | "/";
      query: string;
      startIndex: number;
      selectedIndex: number;
    }>
  >,
) {
  setMentionState({
    isOpen: false,
    trigger: "@",
    query: "",
    startIndex: -1,
    selectedIndex: 0,
  });
}
