import { describe, it, expect } from 'vitest';
import { RecipeFormData, recipeFormSchema } from '../recipeFormSchema';

describe('recipeFormSchema', () => {
  const validFormData: RecipeFormData = {
    title: 'Test Recipe Title',
    description: 'Test Description that is long enough to pass validation',
    instructions: 'Test instructions that are long enough to pass the minimum length validation',
    prompt: 'Test prompt',
    activities: ['activity1', 'activity2'],
    parameters: [
      {
        key: 'param1',
        description: 'Test parameter',
        input_type: 'string' as const,
        requirement: 'required' as const,
      },
    ],
    jsonSchema: '{"type": "object"}',
    subRecipes: [],
  };

  describe('Zod Schema Validation', () => {
    describe('Basic Validation', () => {
      it('validates a complete valid form', () => {
        const result = recipeFormSchema.safeParse(validFormData);
        expect(result.success).toBe(true);
        if (result.success) {
          expect(result.data).toEqual(validFormData);
        }
      });

      it('returns validation result with correct structure', () => {
        const result = recipeFormSchema.safeParse(validFormData);
        expect(result).toHaveProperty('success');
        expect(typeof result.success).toBe('boolean');
        if (result.success) {
          expect(result).toHaveProperty('data');
        } else {
          expect(result).toHaveProperty('error');
        }
      });
    });

    describe('Required Field Validation', () => {
      it('requires title with minimum length', () => {
        const invalidData = { ...validFormData, title: '' };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.some((issue) => issue.path.includes('title'))).toBe(true);
        }
      });

      it('requires description with minimum length', () => {
        const invalidData = { ...validFormData, description: '' };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.some((issue) => issue.path.includes('description'))).toBe(
            true
          );
        }
      });

      it('requires instructions with minimum length', () => {
        const invalidData = { ...validFormData, instructions: '' };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.some((issue) => issue.path.includes('instructions'))).toBe(
            true
          );
        }
      });

      it('allows empty prompt', () => {
        const validData = { ...validFormData, prompt: '' };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });

      it('allows undefined prompt', () => {
        const validData = { ...validFormData, prompt: undefined };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });
    });

    describe('String Field Validation', () => {
      it('validates minimum title length', () => {
        const invalidData = { ...validFormData, title: 'AB' }; // Less than 3 chars
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          const titleError = result.error.issues.find((issue) => issue.path.includes('title'));
          expect(titleError?.message).toContain('at least 3 characters');
        }
      });

      it('validates minimum description length', () => {
        const invalidData = { ...validFormData, description: 'Short' }; // Less than 10 chars
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          const descError = result.error.issues.find((issue) => issue.path.includes('description'));
          expect(descError?.message).toContain('at least 10 characters');
        }
      });

      it('validates minimum instructions length', () => {
        const invalidData = { ...validFormData, instructions: 'Short' }; // Less than 20 chars
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          const instError = result.error.issues.find((issue) =>
            issue.path.includes('instructions')
          );
          expect(instError?.message).toContain('at least 20 characters');
        }
      });

      it('validates maximum title length', () => {
        const longTitle = 'a'.repeat(101); // More than 100 chars
        const invalidData = { ...validFormData, title: longTitle };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          const titleError = result.error.issues.find((issue) => issue.path.includes('title'));
          expect(titleError?.message).toContain('100 characters or less');
        }
      });

      it('validates maximum description length', () => {
        const longDescription = 'a'.repeat(501); // More than 500 chars
        const invalidData = { ...validFormData, description: longDescription };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          const descError = result.error.issues.find((issue) => issue.path.includes('description'));
          expect(descError?.message).toContain('500 characters or less');
        }
      });
    });

    describe('JSON Schema Validation', () => {
      it('validates valid JSON schema', () => {
        const validData = {
          ...validFormData,
          jsonSchema: '{"type": "object", "properties": {"name": {"type": "string"}}}',
        };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });

      it('allows empty JSON schema', () => {
        const validData = { ...validFormData, jsonSchema: '' };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });

      it('allows undefined JSON schema', () => {
        const validData = { ...validFormData, jsonSchema: undefined };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });
    });

    describe('Parameter Validation', () => {
      it('validates parameters with all required fields', () => {
        const validData = {
          ...validFormData,
          parameters: [
            {
              key: 'param1',
              description: 'Test parameter',
              input_type: 'string' as const,
              requirement: 'required' as const,
            },
          ],
        };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });

      it('rejects parameters with empty keys', () => {
        const invalidData = {
          ...validFormData,
          parameters: [
            {
              key: '',
              description: 'Empty key',
              input_type: 'string' as const,
              requirement: 'required' as const,
            },
          ],
        };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(
            result.error.issues.some(
              (issue) => issue.path.includes('parameters') && issue.path.includes('key')
            )
          ).toBe(true);
        }
      });

      it('rejects parameters with empty descriptions', () => {
        const invalidData = {
          ...validFormData,
          parameters: [
            {
              key: 'param1',
              description: '',
              input_type: 'string' as const,
              requirement: 'required' as const,
            },
          ],
        };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(
            result.error.issues.some(
              (issue) => issue.path.includes('parameters') && issue.path.includes('description')
            )
          ).toBe(true);
        }
      });

      it('allows empty parameters array', () => {
        const validData = { ...validFormData, parameters: [] };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });
    });

    describe('Activities Validation', () => {
      it('allows empty activities array', () => {
        const validData = { ...validFormData, activities: [] };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });

      it('allows activities with string content', () => {
        const validData = {
          ...validFormData,
          activities: [
            'Simple activity',
            'Activity with {{parameter}}',
            'Activity with special chars !@#$%',
          ],
        };
        const result = recipeFormSchema.safeParse(validData);
        expect(result.success).toBe(true);
      });

      it('rejects non-string activities', () => {
        const invalidData = {
          ...validFormData,
          activities: [123 as unknown as string, 'valid activity'],
        };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.some((issue) => issue.path.includes('activities'))).toBe(true);
        }
      });
    });

    describe('Multiple Validation Errors', () => {
      it('handles multiple validation errors', () => {
        const invalidData = {
          ...validFormData,
          title: 'AB', // Too short
          description: 'Short', // Too short
          instructions: 'Short', // Too short
        };
        const result = recipeFormSchema.safeParse(invalidData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.length).toBeGreaterThan(0);
          expect(result.error.issues.some((issue) => issue.path.includes('title'))).toBe(true);
          expect(result.error.issues.some((issue) => issue.path.includes('description'))).toBe(
            true
          );
          expect(result.error.issues.some((issue) => issue.path.includes('instructions'))).toBe(
            true
          );
        }
      });
    });

    describe('Edge Cases', () => {
      it('handles null values gracefully', () => {
        const dataWithNulls = {
          ...validFormData,
          title: null as unknown as string,
          description: null as unknown as string,
          instructions: null as unknown as string,
        };
        const result = recipeFormSchema.safeParse(dataWithNulls);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.some((issue) => issue.path.includes('title'))).toBe(true);
          expect(result.error.issues.some((issue) => issue.path.includes('description'))).toBe(
            true
          );
          expect(result.error.issues.some((issue) => issue.path.includes('instructions'))).toBe(
            true
          );
        }
      });

      it('handles undefined values gracefully', () => {
        const dataWithUndefined = {
          ...validFormData,
          title: undefined as unknown as string,
          description: undefined as unknown as string,
          instructions: undefined as unknown as string,
        };
        const result = recipeFormSchema.safeParse(dataWithUndefined);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.some((issue) => issue.path.includes('title'))).toBe(true);
          expect(result.error.issues.some((issue) => issue.path.includes('description'))).toBe(
            true
          );
          expect(result.error.issues.some((issue) => issue.path.includes('instructions'))).toBe(
            true
          );
        }
      });

      it('handles completely empty form data', () => {
        const emptyData = {
          title: '',
          description: '',
          instructions: '',
          prompt: '',
          activities: [],
          parameters: [],
          jsonSchema: '',
          recipeName: '',
          global: true,
        };
        const result = recipeFormSchema.safeParse(emptyData);
        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error.issues.some((issue) => issue.path.includes('title'))).toBe(true);
          expect(result.error.issues.some((issue) => issue.path.includes('description'))).toBe(
            true
          );
          expect(result.error.issues.some((issue) => issue.path.includes('instructions'))).toBe(
            true
          );
        }
      });

      it('handles minimal valid form data', () => {
        const minimalData = {
          title: 'Valid Title',
          description: 'Valid description that meets minimum length',
          instructions: 'Valid instructions that meet the minimum length requirement',
          prompt: '',
          activities: [],
          parameters: [],
          jsonSchema: '',
          recipeName: '',
          global: true,
        };
        const result = recipeFormSchema.safeParse(minimalData);
        expect(result.success).toBe(true);
      });
    });
  });
});
