import { invoke } from "@tauri-apps/api/core";
import type { DistroBundleInfo } from "@/shared/types/distro";

export async function getDistroBundle(): Promise<DistroBundleInfo> {
  return invoke("get_distro_bundle");
}
