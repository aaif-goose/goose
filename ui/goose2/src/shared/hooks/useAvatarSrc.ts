import { useEffect, useState } from "react";
import { resolveAvatarSrc } from "@/shared/lib/avatarUrl";
import type { Avatar } from "@/shared/types/agents";

/**
 * React hook that resolves an Avatar to a displayable image URL.
 * Handles the async avatars-dir lookup internally.
 */
export function useAvatarSrc(
  avatar: Avatar | null | undefined,
): string | undefined {
  const [src, setSrc] = useState<string | undefined>(undefined);

  useEffect(() => {
    let cancelled = false;
    if (!avatar) {
      setSrc(undefined);
      return;
    }
    resolveAvatarSrc(avatar).then((resolved) => {
      if (!cancelled) setSrc(resolved);
    });
    return () => {
      cancelled = true;
    };
  }, [avatar]);

  return src;
}
