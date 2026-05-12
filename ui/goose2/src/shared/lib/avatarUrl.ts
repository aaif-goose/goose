import { convertFileSrc } from "@tauri-apps/api/core";
import type { Avatar } from "@/shared/types/agents";

function resolveFileUrl(value: string): string {
  try {
    return convertFileSrc(decodeURIComponent(new URL(value).pathname));
  } catch {
    return value;
  }
}

export async function resolveAvatarSrc(
  avatar: Avatar | null | undefined,
): Promise<string | undefined> {
  if (!avatar) return undefined;
  if (avatar.type === "url") {
    return avatar.value.startsWith("file://")
      ? resolveFileUrl(avatar.value)
      : avatar.value;
  }
  return undefined;
}
