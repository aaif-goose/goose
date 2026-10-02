import { describe, it, expect } from 'vitest';
import { getRecipeJsonSchema } from './validation';

describe('Recipe Validation', () => {
  describe('getRecipeJsonSchema', () => {
    it('includes standard JSON Schema properties', () => {
      const schema = getRecipeJsonSchema();

      expect(schema.$schema).toBe('http://json-schema.org/draft-07/schema#');
      expect(schema.type).toBe('object');
      expect(schema.title).toBeDefined();
      expect(schema.description).toBeDefined();
    });

    it('documents only ACP-supported recipe extension variants', () => {
      const schemaJson = JSON.stringify(getRecipeJsonSchema());

      expect(schemaJson).toContain('builtin');
      expect(schemaJson).toContain('platform');
      expect(schemaJson).toContain('stdio');
      expect(schemaJson).toContain('streamable_http');
      expect(schemaJson).not.toContain('sse');
      expect(schemaJson).not.toContain('frontend');
      expect(schemaJson).not.toContain('inline_python');
      expect(schemaJson).toContain('available_tools');
    });
  });
});
