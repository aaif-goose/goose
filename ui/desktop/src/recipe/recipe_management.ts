import { Recipe, saveRecipe as saveRecipeApi, listRecipes, RecipeManifest } from '../api';
import { stripEmptyExtensions } from '.';

export const saveRecipe = async (
  recipe: Recipe,
  recipeId?: string | null
): Promise<{ id: string; fileName: string; filePath: string }> => {
  try {
    const response = await saveRecipeApi({
      body: {
        recipe: stripEmptyExtensions(recipe),
        id: recipeId,
      },
      throwOnError: true,
    });
    return {
      id: response.data.id,
      fileName: response.data.file_name,
      filePath: response.data.file_path,
    };
  } catch (error) {
    let error_message = 'unknown error';
    if (typeof error === 'object' && error !== null && 'message' in error) {
      error_message = error.message as string;
    }
    throw new Error(error_message);
  }
};

export const listSavedRecipes = async (): Promise<RecipeManifest[]> => {
  try {
    const listRecipeResponse = await listRecipes();
    return listRecipeResponse?.data?.manifests ?? [];
  } catch (error) {
    console.warn('Failed to list saved recipes:', error);
    return [];
  }
};

const parseLastModified = (val: string | Date): Date => {
  return val instanceof Date ? val : new Date(val);
};

export const convertToLocaleDateString = (lastModified: string): string => {
  if (lastModified) {
    return parseLastModified(lastModified).toLocaleDateString();
  }
  return '';
};

export const getStorageDirectory = (isGlobal: boolean): string => {
  if (isGlobal) {
    const pathRoot = window.appConfig.get('GOOSE_PATH_ROOT') as string | undefined;
    if (pathRoot) {
      return `${pathRoot}/config/recipes`;
    }
    const configDir = window.appConfig.get('GOOSE_CONFIG_DIR') as string | undefined;
    if (configDir) {
      return `${configDir}/recipes`;
    }
    return '~/.config/goose/recipes';
  } else {
    // For directory recipes, build absolute path using working directory
    const workingDir = window.appConfig.get('GOOSE_WORKING_DIR') as string;
    return `${workingDir}/.goose/recipes`;
  }
};
