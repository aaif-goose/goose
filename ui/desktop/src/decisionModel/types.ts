/**
 * A question asking which option best fits the state.
 *
 * `options` maps each selectable option to a rubric describing it, or null when
 * the option name speaks for itself.
 */
export type ChoiceQuestion = {
  instructions: string;
  options: Record<string, string | null>;
};

export type ChoiceAnswer = {
  /** The selected option; always one of the question's option keys. */
  choice: string;
  /** How sure the model is, 0 to 1. Callers should gate low-confidence answers. */
  confidence: number;
};
