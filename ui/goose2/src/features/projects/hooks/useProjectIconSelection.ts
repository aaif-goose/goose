import { useCallback, useEffect, useMemo, useState } from "react";
import {
  readProjectIcon,
  scanProjectIcons,
  type ProjectIconCandidate,
} from "../api/projects";
import {
  DEFAULT_PROJECT_ICON,
  normalizeProjectIcon,
} from "../lib/projectIcons";
import { parseEditorText } from "../lib/projectPromptText";

interface ChooseCustomProjectIconOptions {
  title: string;
  filterName: string;
}

export function useProjectIconSelection({
  isOpen,
  prompt,
}: {
  isOpen: boolean;
  prompt: string;
}) {
  const [icon, setIcon] = useState(DEFAULT_PROJECT_ICON);
  const [iconError, setIconError] = useState<string | null>(null);
  const [iconCandidates, setIconCandidates] = useState<ProjectIconCandidate[]>(
    [],
  );
  const [iconScanPending, setIconScanPending] = useState(false);

  const scannedWorkingDirKey = useMemo(
    () => parseEditorText(prompt).workingDirs.join("\n"),
    [prompt],
  );

  useEffect(() => {
    const workingDirs = scannedWorkingDirKey.split("\n").filter(Boolean);
    if (!isOpen || workingDirs.length === 0) {
      setIconCandidates([]);
      setIconScanPending(false);
      return;
    }

    let active = true;
    setIconScanPending(true);
    const timeout = window.setTimeout(() => {
      scanProjectIcons(workingDirs)
        .then((candidates) => {
          if (active) {
            setIconCandidates(candidates);
          }
        })
        .catch(() => {
          if (active) {
            setIconCandidates([]);
          }
        })
        .finally(() => {
          if (active) {
            setIconScanPending(false);
          }
        });
    }, 250);

    return () => {
      active = false;
      window.clearTimeout(timeout);
    };
  }, [isOpen, scannedWorkingDirKey]);

  const resetIcon = useCallback((nextIcon?: string | null) => {
    setIcon(normalizeProjectIcon(nextIcon));
    setIconError(null);
  }, []);

  const chooseIcon = useCallback((nextIcon: string) => {
    setIcon(nextIcon);
    setIconError(null);
  }, []);

  const chooseCustomIcon = useCallback(
    async ({ title, filterName }: ChooseCustomProjectIconOptions) => {
      try {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const selected = await open({
          directory: false,
          multiple: false,
          title,
          filters: [
            {
              name: filterName,
              extensions: ["svg", "png", "ico", "jpg", "jpeg", "webp"],
            },
          ],
        });
        if (selected && typeof selected === "string") {
          const iconData = await readProjectIcon(selected);
          setIcon(iconData.icon);
          setIconError(null);
        }
      } catch (err) {
        setIconError(String(err));
      }
    },
    [],
  );

  return {
    icon,
    iconCandidates,
    iconScanPending,
    iconError,
    chooseIcon,
    chooseCustomIcon,
    resetIcon,
  };
}
