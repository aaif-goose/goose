import { revealItemInDir } from "@tauri-apps/plugin-opener";

export async function revealInFileManager(path: string): Promise<void> {
  await revealItemInDir(path);
}
