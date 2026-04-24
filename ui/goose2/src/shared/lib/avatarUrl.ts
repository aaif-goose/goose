import { convertFileSrc } from "@tauri-apps/api/core";
import { getAvatarsDir } from "@/shared/api/agents";
import type { Avatar } from "@/shared/types/agents";

let cachedAvatarsDir: string | null = null;

async function ensureAvatarsDir(): Promise<string> {
  if (!cachedAvatarsDir) {
    cachedAvatarsDir = await getAvatarsDir();
  }
  return cachedAvatarsDir;
}

/**
 * Resolve an Avatar to a displayable image URL.
 * Lazily fetches the avatars directory on first call for a local avatar.
 */
export async function resolveAvatarSrc(
  avatar: Avatar | null | undefined,
): Promise<string | undefined> {
  if (!avatar) return undefined;
  if (avatar.type === "url") return avatar.value;
  if (avatar.type === "local") {
    const dir = await ensureAvatarsDir();
    return convertFileSrc(`${dir}/${avatar.value}`);
  }
  return undefined;
}
