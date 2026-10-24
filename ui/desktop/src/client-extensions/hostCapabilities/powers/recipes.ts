import { listRecipes } from '../../../acp/recipe';
import type { HostCapabilityDefinition } from '../types';

export const recipesPower: HostCapabilityDefinition = {
  id: 'recipes',
  description: 'List the recipe library.',
  methods: {
    list: {
      permission: 'recipes:read',
      handle: async () => {
        const recipes = await listRecipes();
        return recipes.map((entry) => ({
          id: entry.id,
          title: entry.recipe.title,
          description: entry.recipe.description,
          lastModified: entry.last_modified,
          scheduleCron: entry.schedule_cron ?? null,
          slashCommand: entry.slash_command ?? null,
        }));
      },
    },
  },
};
