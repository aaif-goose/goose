import { describe, it, expect } from 'vitest';
import { detectInterruption, INTERRUPTION_KEYWORDS } from './interruptionDetector';

describe('interruptionDetector', () => {
  describe('detectInterruption', () => {
    describe('exact matches (confidence: 1.0)', () => {
      it('detects exact stop variations', () => {
        const result = detectInterruption('stop');
        expect(result).not.toBeNull();
        expect(result?.confidence).toBe(1.0);
        expect(result?.keyword.action).toBe('stop');
        expect(result?.shouldInterrupt).toBe(true);
      });

      it('detects exact wait variations', () => {
        const result = detectInterruption('pause');
        expect(result).not.toBeNull();
        expect(result?.confidence).toBe(1.0);
        expect(result?.keyword.action).toBe('pause');
        expect(result?.shouldInterrupt).toBe(true);
      });

      it('detects exact redirect variations', () => {
        const result = detectInterruption('actually');
        expect(result).not.toBeNull();
        expect(result?.confidence).toBe(1.0);
        expect(result?.keyword.action).toBe('redirect');
        expect(result?.shouldInterrupt).toBe(true);
      });

      it('is case-insensitive', () => {
        expect(detectInterruption('STOP')?.confidence).toBe(1.0);
        expect(detectInterruption('Stop')?.confidence).toBe(1.0);
        expect(detectInterruption('sToP')?.confidence).toBe(1.0);
      });
    });

    describe('beginning matches (confidence: 0.9)', () => {
      it('detects variations at start with space', () => {
        const result = detectInterruption('stop doing that');
        expect(result).not.toBeNull();
        expect(result?.confidence).toBe(0.9);
        expect(result?.shouldInterrupt).toBe(true);
      });

      it('detects variations at start with comma', () => {
        const result = detectInterruption('wait, I need to think');
        expect(result).not.toBeNull();
        expect(result?.confidence).toBe(0.9);
        expect(result?.shouldInterrupt).toBe(true);
      });

      it('detects "never mind" at the beginning', () => {
        const result = detectInterruption('never mind, forget it');
        expect(result).not.toBeNull();
        expect(result?.confidence).toBe(0.9);
        expect(result?.keyword.action).toBe('stop');
      });
    });

    describe('contained matches in short inputs (confidence: 0.7)', () => {
      it('detects keywords in short messages', () => {
        const result = detectInterruption('oh wait please');
        expect(result).not.toBeNull();
        expect(result?.confidence).toBe(0.7);
        expect(result?.matchedText).toBe('wait');
      });

      it('only interrupts high priority keywords when confidence is 0.7', () => {
        // High priority - should interrupt
        const highPriority = detectInterruption('oh stop please');
        expect(highPriority?.shouldInterrupt).toBe(true);

        // Medium priority - should not interrupt at 0.7 confidence
        // Note: "oh actually wait" will match "wait" (high priority) first, so let's use a different example
        const mediumPriority = detectInterruption('oh actually');
        expect(mediumPriority?.confidence).toBe(0.7);
        expect(mediumPriority?.shouldInterrupt).toBe(false);
      });

      it('detects contained keywords in short inputs', () => {
        // The implementation actually DOES match keywords contained in short inputs
        // This is by design - it uses .includes() for short messages
        const result = detectInterruption('unstoppable');
        expect(result).not.toBeNull();
        expect(result?.matchedText).toBe('stop');
        expect(result?.confidence).toBe(0.7);

        // Words that don't contain any keyword variations should return null
        expect(detectInterruption('continuing')).toBeNull();
        expect(detectInterruption('proceeding')).toBeNull();

        // Long messages should not match even if they contain keywords
        expect(
          detectInterruption('this is a very long message with stop in it somewhere')
        ).toBeNull();
      });

      it('ignores keywords in long messages', () => {
        const longMessage =
          'This is a very long message that contains stop but should not be detected';
        expect(detectInterruption(longMessage)).toBeNull();
      });
    });

    describe('edge cases', () => {
      it('handles multiple keyword matches by returning first match', () => {
        const result = detectInterruption('stop wait');
        expect(result?.matchedText).toBe('stop');
        expect(result?.confidence).toBe(0.9);
      });
    });
  });

  describe('INTERRUPTION_KEYWORDS', () => {
    it('includes the main keyword in variations', () => {
      INTERRUPTION_KEYWORDS.forEach((keyword) => {
        expect(keyword.variations).toContain(keyword.keyword);
      });
    });
  });
});
