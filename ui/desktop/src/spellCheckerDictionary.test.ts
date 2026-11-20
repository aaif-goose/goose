import { expect, it, vi } from 'vitest';
import { addSharedDictionaryWord, copySharedDictionaryWords } from './spellCheckerDictionary';

function dictionary(...initialWords: string[]) {
  const words = new Set(initialWords);
  return {
    listWordsInSpellCheckerDictionary: async () => [...words],
    addWordToSpellCheckerDictionary: vi.fn((word: string) => {
      if (words.has(word)) return false;
      words.add(word);
      return true;
    }),
  };
}

it('preserves legacy words and shares additions with open and future windows', async () => {
  const shared = dictionary('legacy-word');
  const first = dictionary('legacy-word');
  const second = dictionary();
  await copySharedDictionaryWords(shared, first);
  await copySharedDictionaryWords(shared, second);
  expect(first.addWordToSpellCheckerDictionary).not.toHaveBeenCalled();
  expect(await second.listWordsInSpellCheckerDictionary()).toEqual(['legacy-word']);

  addSharedDictionaryWord(shared, [first, second, first], 'new-word');
  const third = dictionary();
  await copySharedDictionaryWords(shared, third);

  expect(first.addWordToSpellCheckerDictionary).toHaveBeenCalledTimes(1);
  for (const session of [shared, first, second, third]) {
    expect(await session.listWordsInSpellCheckerDictionary()).toEqual(['legacy-word', 'new-word']);
  }
});
