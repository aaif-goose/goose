import { invoke } from "@tauri-apps/api/core";

export interface Recipe {
  id: string;
  name: string;
  title: string | null;
  description: string | null;
  path: string;
}

export async function listRecipes(): Promise<Recipe[]> {
  return invoke<Recipe[]>("list_recipes");
}

export async function loadRecipePrompt(path: string): Promise<string> {
  return invoke<string>("load_recipe_prompt", { path });
}
