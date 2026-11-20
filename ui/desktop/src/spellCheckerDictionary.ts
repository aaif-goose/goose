import type { Session } from 'electron';

type SpellCheckerDictionary = Pick<
  Session,
  'listWordsInSpellCheckerDictionary' | 'addWordToSpellCheckerDictionary'
>;

export async function copySharedDictionaryWords(
  shared: SpellCheckerDictionary,
  target: SpellCheckerDictionary
): Promise<void> {
  const targetWords = new Set(await target.listWordsInSpellCheckerDictionary());
  for (const word of await shared.listWordsInSpellCheckerDictionary()) {
    if (!targetWords.has(word)) target.addWordToSpellCheckerDictionary(word);
  }
}

export function addSharedDictionaryWord(
  shared: SpellCheckerDictionary,
  targets: SpellCheckerDictionary[],
  word: string
): void {
  for (const dictionary of new Set([shared, ...targets])) {
    dictionary.addWordToSpellCheckerDictionary(word);
  }
}
