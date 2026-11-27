export interface StartRecipePayload {
  recipeDeeplink: string;
  recipeParameters: Record<string, string>;
  scheduledJobId?: string;
}
