import type { Recipe } from '../../recipe';

interface RecipePreviewProps {
  recipe: Recipe;
}

export function RecipePreview({ recipe }: RecipePreviewProps) {
  return (
    <pre
      data-testid="recipe-preview"
      className="overflow-x-auto whitespace-pre-wrap break-words text-xs text-text-primary"
    >
      {JSON.stringify(recipe, null, 2)}
    </pre>
  );
}
