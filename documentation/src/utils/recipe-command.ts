export function buildRecipeCliCommand(
  localPath: string,
  filledParams = "",
): string | null {
  if (!localPath.endsWith(".yaml") && !localPath.endsWith(".json")) {
    return null;
  }

  return `goose run --recipe ${localPath}${filledParams ? ` --params ${filledParams}` : ""}`;
}
