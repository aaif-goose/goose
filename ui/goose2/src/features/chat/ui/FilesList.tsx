import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  FileTree,
  FileTreeFile,
  FileTreeFolder,
} from "@/shared/ui/ai-elements/file-tree";
import { listDirectoryEntries, type FileTreeEntry } from "@/shared/api/system";

interface FilesListProps {
  projectWorkingDirs?: string[];
}

interface DirectoryState {
  entries: FileTreeEntry[];
  error: string | null;
  status: "idle" | "loading" | "loaded" | "error";
}

const EMPTY_ROOTS: string[] = [];

function basename(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function normalizeRoots(roots: string[]): string[] {
  return Array.from(
    new Set(roots.map((root) => root.trim()).filter((root) => root.length > 0)),
  );
}

function TreeStatusRow({
  destructive = false,
  text,
}: {
  destructive?: boolean;
  text: string;
}) {
  return (
    <div
      className={`px-2 py-1 text-xs ${
        destructive ? "text-destructive" : "text-muted-foreground"
      }`}
    >
      {text}
    </div>
  );
}

interface DirectoryNodeProps {
  directoryStates: Record<string, DirectoryState>;
  entry: FileTreeEntry;
  loadDirectory: (path: string) => void;
  t: (key: string) => string;
}

function DirectoryNode({
  directoryStates,
  entry,
  loadDirectory,
  t,
}: DirectoryNodeProps) {
  if (entry.kind === "file") {
    return (
      <FileTreeFile
        path={entry.path}
        name={entry.name}
        contextMenuPath={entry.path}
        title={entry.path}
      />
    );
  }

  return (
    <FileTreeFolder
      path={entry.path}
      name={entry.name}
      contextMenuPath={entry.path}
      title={entry.path}
      toggleOnSelect
    >
      <DirectoryChildren
        directoryPath={entry.path}
        directoryStates={directoryStates}
        loadDirectory={loadDirectory}
        t={t}
      />
    </FileTreeFolder>
  );
}

function DirectoryChildren({
  directoryPath,
  directoryStates,
  loadDirectory,
  t,
}: {
  directoryPath: string;
  directoryStates: Record<string, DirectoryState>;
  loadDirectory: (path: string) => void;
  t: (key: string) => string;
}) {
  const state = directoryStates[directoryPath];

  useEffect(() => {
    if (!state || state.status === "idle") {
      loadDirectory(directoryPath);
    }
  }, [directoryPath, loadDirectory, state]);

  if (!state || state.status === "idle" || state.status === "loading") {
    return <TreeStatusRow text={t("files.loading")} />;
  }

  if (state.status === "error") {
    return <TreeStatusRow destructive text={t("files.loadError")} />;
  }

  if (state.entries.length === 0) {
    return <TreeStatusRow text={t("files.folderEmpty")} />;
  }

  return (
    <>
      {state.entries.map((entry) => (
        <DirectoryNode
          key={entry.path}
          directoryStates={directoryStates}
          entry={entry}
          loadDirectory={loadDirectory}
          t={t}
        />
      ))}
    </>
  );
}

export function FilesList({ projectWorkingDirs }: FilesListProps) {
  const { t } = useTranslation("chat");
  const roots = useMemo(
    () => normalizeRoots(projectWorkingDirs ?? EMPTY_ROOTS),
    [projectWorkingDirs],
  );
  const rootsKey = useMemo(() => roots.join("\n"), [roots]);
  const [directoryStates, setDirectoryStates] = useState<
    Record<string, DirectoryState>
  >({});
  const directoryStatesRef = useRef<Record<string, DirectoryState>>({});
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(
    () => new Set(roots),
  );
  const [selectedPath, setSelectedPath] = useState<string>();
  const generationRef = useRef(0);

  const loadDirectory = useCallback((path: string) => {
    const generation = generationRef.current;
    const existing = directoryStatesRef.current[path];
    if (
      existing &&
      (existing.status === "loading" || existing.status === "loaded")
    ) {
      return;
    }

    directoryStatesRef.current = {
      ...directoryStatesRef.current,
      [path]: {
        entries: [],
        error: null,
        status: "loading",
      },
    };
    setDirectoryStates(directoryStatesRef.current);

    void listDirectoryEntries(path)
      .then((entries) => {
        if (generationRef.current !== generation) {
          return;
        }

        directoryStatesRef.current = {
          ...directoryStatesRef.current,
          [path]: {
            entries,
            error: null,
            status: "loaded",
          },
        };
        setDirectoryStates(directoryStatesRef.current);
      })
      .catch((error: unknown) => {
        if (generationRef.current !== generation) {
          return;
        }

        directoryStatesRef.current = {
          ...directoryStatesRef.current,
          [path]: {
            entries: [],
            error: error instanceof Error ? error.message : String(error),
            status: "error",
          },
        };
        setDirectoryStates(directoryStatesRef.current);
      });
  }, []);

  useEffect(() => {
    const nextRoots = rootsKey ? rootsKey.split("\n") : [];
    generationRef.current += 1;
    directoryStatesRef.current = {};
    setDirectoryStates({});
    setExpandedPaths(new Set(nextRoots));
    setSelectedPath(undefined);
  }, [rootsKey]);

  useEffect(() => {
    const nextRoots = rootsKey ? rootsKey.split("\n") : [];
    for (const root of nextRoots) {
      loadDirectory(root);
    }
  }, [loadDirectory, rootsKey]);

  const handleExpandedChange = useCallback(
    (nextExpanded: Set<string>) => {
      const normalizedExpanded = new Set(nextExpanded);
      setExpandedPaths(normalizedExpanded);

      for (const path of normalizedExpanded) {
        loadDirectory(path);
      }
    },
    [loadDirectory],
  );

  const handleOpenFile = useCallback((path: string) => {
    setSelectedPath(path);
    void openPath(path);
  }, []);

  if (roots.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center px-4 text-center">
        <p className="text-sm text-muted-foreground">{t("files.empty")}</p>
      </div>
    );
  }

  return (
    <div className="min-w-0 px-1 pb-1 pt-1">
      <FileTree
        className="border-0 bg-transparent"
        contentClassName="p-1"
        expanded={expandedPaths}
        onExpandedChange={handleExpandedChange}
        onSelect={handleOpenFile}
        selectedPath={selectedPath}
      >
        {roots.map((root) => {
          const state = directoryStates[root];
          return (
            <FileTreeFolder
              key={root}
              path={root}
              name={basename(root)}
              contextMenuPath={root}
              title={root}
              toggleOnSelect
            >
              {state?.status === "error" ? (
                <TreeStatusRow destructive text={t("files.rootLoadError")} />
              ) : (
                <DirectoryChildren
                  directoryPath={root}
                  directoryStates={directoryStates}
                  loadDirectory={loadDirectory}
                  t={t}
                />
              )}
            </FileTreeFolder>
          );
        })}
      </FileTree>
    </div>
  );
}
