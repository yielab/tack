import type { DecisionOption, DecisionRecord } from '../execution';

/** The id of the way out: a question's own refusal, drawn as a plain action beside the answer, not as a choice. */
const DECLINE_ID = 'decline';

/** Where a question takes text: the field's name, whether it must be filled, and the choices it opens under. */
export interface AnswerText {
  label: string;
  required: boolean;
  options: string[];
}

/** What the runner said about the text a question takes (`metadata.text`); nothing when it takes none. */
export function answerTextOf(metadata: unknown): AnswerText | null {
  const raw = metadata && typeof metadata === 'object' ? (metadata as Record<string, unknown>).text : null;
  if (!raw || typeof raw !== 'object') return null;
  const { label, required, options } = raw as Record<string, unknown>;
  const ids = Array.isArray(options) ? options.filter((id): id is string => typeof id === 'string') : [];
  if (ids.length === 0) return null;
  return { label: typeof label === 'string' && label !== '' ? label : 'Your answer', required: required === true, options: ids };
}

export interface AnswerChoices {
  /** What the person picks between, the recommended one first. */
  choices: DecisionOption[];
  /** The way out, when the question offers one. */
  decline: DecisionOption | null;
  /** The recommended choice, when the question names one among those offered. */
  recommendedId: string;
}

export function answerChoices(decision: DecisionRecord): AnswerChoices {
  const decline = decision.options.find((o) => o.option_id === DECLINE_ID) ?? null;
  const rest = decision.options.filter((o) => o.option_id !== DECLINE_ID);
  const named = decision.recommendation?.option_id;
  const recommendedId = named && rest.some((o) => o.option_id === named) ? named : '';
  const choices = recommendedId
    ? [...rest.filter((o) => o.option_id === recommendedId), ...rest.filter((o) => o.option_id !== recommendedId)]
    : rest;
  return { choices, decline, recommendedId };
}
