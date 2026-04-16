import { useState, useCallback } from "react";
import { isExternalHref } from "@/features/chat/lib/artifactPathPolicy";
import { useArtifactPolicyContext } from "@/features/chat/hooks/ArtifactPolicyContext";

/**
 * Delegated click handler that intercepts local link clicks within a
 * container and routes them through the artifact policy layer.
 *
 * External links are intentionally not handled here — Streamdown's
 * linkSafety modal shows a confirmation dialog before opening them.
 */
export function useArtifactLinkHandler() {
  const { resolveMarkdownHref, openResolvedPath } = useArtifactPolicyContext();
  const [pathNotice, setPathNotice] = useState<string | null>(null);

  const handleContentClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      const anchor = (event.target as HTMLElement).closest("a");
      if (!anchor) return;
      const href = anchor.getAttribute("href");
      if (!href) return;

      if (isExternalHref(href)) return;

      event.preventDefault();
      const resolved = resolveMarkdownHref(href);
      if (!resolved) return;

      if (!resolved.allowed) {
        setPathNotice(
          resolved.blockedReason ||
            "Path is outside allowed project/artifacts roots.",
        );
        return;
      }

      setPathNotice(null);
      void openResolvedPath(resolved.resolvedPath).catch((err) => {
        setPathNotice(err instanceof Error ? err.message : String(err));
      });
    },
    [resolveMarkdownHref, openResolvedPath],
  );

  return { handleContentClick, pathNotice };
}
