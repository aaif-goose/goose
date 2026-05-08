import { invoke } from "@tauri-apps/api/core";

export interface ZipArchiveResult {
  data: string;
  filename: string;
}

export async function createZipArchive(
  sourcePath: string,
): Promise<ZipArchiveResult> {
  return invoke("create_zip_archive", { sourcePath });
}
