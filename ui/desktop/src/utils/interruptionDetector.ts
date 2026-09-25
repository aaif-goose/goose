/**
 * Utility for detecting interruption keywords in user input
 */

interface InterruptionKeyword {
  keyword: string;
  variations: string[];
  priority: 'high' | 'medium' | 'low';
  action: 'stop' | 'pause' | 'redirect';
}

// Define interruption keywords and their variations
export const INTERRUPTION_KEYWORDS: InterruptionKeyword[] = [
  {
    keyword: 'stop',
    variations: ['stop', 'halt', 'cease', 'quit', 'end', 'abort', 'cancel'],
    priority: 'high',
    action: 'stop',
  },
  {
    keyword: 'wait',
    variations: ['wait', 'hold', 'pause', 'hold on', 'wait up', 'hold up'],
    priority: 'high',
    action: 'pause',
  },
  {
    keyword: 'no',
    variations: ['no', 'nope', 'nah', 'wrong', 'incorrect', 'not right'],
    priority: 'medium',
    action: 'stop',
  },
  {
    keyword: 'actually',
    variations: ['actually', 'instead', 'rather', 'better idea', 'change of plans'],
    priority: 'medium',
    action: 'redirect',
  },
  {
    keyword: 'nevermind',
    variations: ['nevermind', 'never mind', 'forget it', 'ignore that', 'disregard'],
    priority: 'medium',
    action: 'stop',
  },
];

export interface InterruptionMatch {
  keyword: InterruptionKeyword;
  matchedText: string;
  confidence: number;
  shouldInterrupt: boolean;
}

/**
 * Analyzes input text for interruption keywords
 */
export function detectInterruption(input: string): InterruptionMatch | null {
  if (!input || input.trim().length === 0) {
    return null;
  }

  const normalizedInput = input.toLowerCase().trim();

  // Check for exact matches first (highest confidence)
  for (const keyword of INTERRUPTION_KEYWORDS) {
    for (const variation of keyword.variations) {
      if (normalizedInput === variation) {
        return {
          keyword,
          matchedText: variation,
          confidence: 1.0,
          shouldInterrupt: true,
        };
      }
    }
  }

  // Check for matches at the beginning of input (high confidence)
  for (const keyword of INTERRUPTION_KEYWORDS) {
    for (const variation of keyword.variations) {
      if (
        normalizedInput.startsWith(variation + ' ') ||
        normalizedInput.startsWith(variation + ',')
      ) {
        return {
          keyword,
          matchedText: variation,
          confidence: 0.9,
          shouldInterrupt: true,
        };
      }
    }
  }

  // Check for matches anywhere in short inputs (medium confidence)
  if (normalizedInput.length <= 20) {
    for (const keyword of INTERRUPTION_KEYWORDS) {
      for (const variation of keyword.variations) {
        if (normalizedInput.includes(variation)) {
          return {
            keyword,
            matchedText: variation,
            confidence: 0.7,
            shouldInterrupt: keyword.priority === 'high',
          };
        }
      }
    }
  }

  return null;
}
