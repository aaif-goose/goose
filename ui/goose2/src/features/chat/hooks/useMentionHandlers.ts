import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listSkills } from "@/features/skills/api/skills";
import {
  expandSkillSlashCommand,
  resolveSkillSlashCommand,
} from "@/features/skills/lib/skillChatPrompt";
import { listFilesForMentions } from "@/shared/api/system";
import type { Persona } from "@/shared/types/agents";
import {
  useMentionDetection,
  type FileMentionItem,
  type MentionItem,
  type SkillMentionItem,
} from "../ui/MentionAutocomplete";
import { useArtifactPolicyContext } from "./ArtifactPolicyContext";

interface MentionHandlersOptions {
  personas: Persona[];
  projectWorkingDirs?: string[] | undefined;
  text: string;
  setText: (value: string) => void;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  onPersonaChange?: ((id: string | null) => void) | undefined;
  onSkillMentionSelect?: (skill: SkillMentionItem) => void;
}

function basename(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function normalizeRoots(roots: string[] | undefined): string[] {
  return Array.from(
    new Set(
      (roots ?? [])
        .map((root) => root.trim())
        .filter((root) => root.length > 0),
    ),
  );
}

function toDisplayPath(path: string, roots: string[]): string {
  const normalizedPath = path.replace(/\\/g, "/");
  for (const root of roots) {
    const normalizedRoot = root.replace(/\\/g, "/").replace(/\/+$/, "");
    const prefix = `${normalizedRoot}/`;
    if (normalizedPath.startsWith(prefix)) {
      const relative = normalizedPath.slice(prefix.length);
      const rootName = basename(normalizedRoot);
      return `${rootName}/${relative}`;
    }
  }
  return path;
}

function sameStringArray(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

/**
 * Combines persona + skill + file mention detection, filtering, and selection handlers.
 * Keeps ChatInput under the file-size limit by centralising mention logic.
 */
export function useMentionHandlers({
  personas,
  projectWorkingDirs,
  text,
  setText,
  textareaRef,
  onPersonaChange,
  onSkillMentionSelect,
}: MentionHandlersOptions) {
  const { getAllSessionArtifacts } = useArtifactPolicyContext();
  const normalizedProjectRoots = useMemo(
    () => normalizeRoots(projectWorkingDirs),
    [projectWorkingDirs],
  );
  const rootsKey = useMemo(
    () => normalizedProjectRoots.join("\n"),
    [normalizedProjectRoots],
  );
  const [projectFilePaths, setProjectFilePaths] = useState<string[]>([]);
  const [skillMentionItems, setSkillMentionItems] = useState<
    SkillMentionItem[]
  >([]);

  useEffect(() => {
    let cancelled = false;

    void listSkills(normalizedProjectRoots)
      .then((skills) => {
        if (cancelled) return;
        setSkillMentionItems(
          skills.map((skill) => ({
            id: skill.id,
            name: skill.name,
            description: skill.description,
            sourceLabel: skill.sourceLabel,
          })),
        );
      })
      .catch((error) => {
        if (cancelled) return;
        console.error("Failed to load skills for mentions:", error);
        setSkillMentionItems([]);
      });

    return () => {
      cancelled = true;
    };
  }, [normalizedProjectRoots]);

  useEffect(() => {
    // Clear stale results immediately so users never see files from the
    // previous project while the new scan is in flight.
    setProjectFilePaths([]);

    if (!rootsKey) {
      return;
    }

    let cancelled = false;

    void listFilesForMentions(normalizedProjectRoots)
      .then((paths) => {
        if (cancelled) return;
        setProjectFilePaths((prev) =>
          sameStringArray(prev, paths) ? prev : paths,
        );
      })
      .catch((error) => {
        if (cancelled) return;
        console.error("Failed to load project files for mentions:", error);
        setProjectFilePaths((prev) => (prev.length === 0 ? prev : []));
      });

    return () => {
      cancelled = true;
    };
  }, [rootsKey, normalizedProjectRoots]);

  const fileMentionItems: FileMentionItem[] = useMemo(() => {
    const dedup = new Map<string, FileMentionItem>();

    for (const artifact of getAllSessionArtifacts()) {
      const key = artifact.resolvedPath.trim().toLowerCase();
      if (!key || dedup.has(key)) continue;
      dedup.set(key, {
        resolvedPath: artifact.resolvedPath,
        displayPath: artifact.displayPath,
        filename: artifact.filename,
        kind: artifact.kind,
      });
    }

    for (const path of projectFilePaths) {
      const key = path.trim().toLowerCase();
      if (!key || dedup.has(key)) continue;
      dedup.set(key, {
        resolvedPath: path,
        displayPath: toDisplayPath(path, normalizedProjectRoots),
        filename: basename(path),
        kind: "file",
      });
    }

    return Array.from(dedup.values());
  }, [getAllSessionArtifacts, projectFilePaths, normalizedProjectRoots]);

  const {
    mentionOpen,
    mentionQuery,
    mentionStartIndex,
    mentionSelectedIndex,
    filteredPersonas,
    filteredSkills,
    filteredFiles,
    detectMention,
    closeMention,
    navigateMention,
    confirmMention,
  } = useMentionDetection(personas, skillMentionItems, fileMentionItems);

  // ---- post-selection cursor placement ------------------------------------
  // After a mention is confirmed we update `text` via setState. A useEffect
  // watches for a pending cursor position and applies focus + cursor once
  // React has flushed the new text into the textarea.

  const pendingCursorRef = useRef<number | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: text triggers the effect after setText flushes
  useEffect(() => {
    if (pendingCursorRef.current == null) return;
    const ta = textareaRef.current;
    if (!ta) return;
    const pos = pendingCursorRef.current;
    pendingCursorRef.current = null;
    ta.focus();
    ta.style.height = "auto";
    ta.style.height = `${Math.min(ta.scrollHeight, 200)}px`;
    ta.setSelectionRange(pos, pos);
  }, [text, textareaRef]);

  // ---- selection handlers ------------------------------------------------

  const handlePersonaMentionSelect = useCallback(
    (persona: Persona) => {
      const before = text.slice(0, mentionStartIndex);
      const after = text.slice(mentionStartIndex + 1 + mentionQuery.length);
      const newText = `${before}${after}`.trimStart();
      pendingCursorRef.current = Math.min(before.length, newText.length);
      setText(newText);
      closeMention();
      onPersonaChange?.(persona.id);
    },
    [
      text,
      mentionStartIndex,
      mentionQuery,
      closeMention,
      onPersonaChange,
      setText,
    ],
  );

  const handleFileMentionSelect = useCallback(
    (file: FileMentionItem) => {
      const before = text.slice(0, mentionStartIndex);
      const after = text.slice(mentionStartIndex + 1 + mentionQuery.length);
      const inserted = file.resolvedPath;
      const newText = `${before}${inserted} ${after}`;
      pendingCursorRef.current = before.length + inserted.length + 1;
      setText(newText);
      closeMention();
    },
    [text, mentionStartIndex, mentionQuery, closeMention, setText],
  );

  const handleSkillMentionSelect = useCallback(
    (skill: SkillMentionItem) => {
      const before = text.slice(0, mentionStartIndex);
      const after = text.slice(mentionStartIndex + 1 + mentionQuery.length);
      const newText = `${before}${after}`.trimStart();
      pendingCursorRef.current = Math.min(before.length, newText.length);
      setText(newText);
      closeMention();
      onSkillMentionSelect?.(skill);
    },
    [
      text,
      mentionStartIndex,
      mentionQuery,
      closeMention,
      onSkillMentionSelect,
      setText,
    ],
  );

  const handleMentionConfirm = useCallback(
    (item: MentionItem) => {
      if (item.type === "persona") {
        handlePersonaMentionSelect(item.persona);
      } else if (item.type === "skill") {
        handleSkillMentionSelect(item.skill);
      } else {
        handleFileMentionSelect(item.file);
      }
    },
    [
      handlePersonaMentionSelect,
      handleSkillMentionSelect,
      handleFileMentionSelect,
    ],
  );

  return {
    fileMentionItems,
    skillMentionItems,
    mentionOpen,
    mentionQuery,
    mentionStartIndex,
    mentionSelectedIndex,
    filteredPersonas,
    filteredSkills,
    filteredFiles,
    expandSkillSlashCommand: (message: string) =>
      expandSkillSlashCommand(message, skillMentionItems),
    resolveSkillSlashCommand: (message: string) =>
      resolveSkillSlashCommand(message, skillMentionItems),
    detectMention,
    closeMention,
    navigateMention,
    confirmMention,
    handlePersonaMentionSelect,
    handleSkillMentionSelect,
    handleFileMentionSelect,
    handleMentionConfirm,
  };
}
