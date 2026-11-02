import { LEVEL_MEANING } from '../help/texts';

// A run's verdict, its assumptions and its questions arrive as text the verifier and the harness wrote
// for themselves: product prefixes, snake_case codes, upper-case headings. The main surface says each
// in one plain sentence and keeps the raw text for a tooltip or the Technical fold; a line this does not
// recognise is shown as it came, never guessed at. No verifier or harness product is named.

/** The words for the codes a verdict is made of. */
const CODE_WORDS: Record<string, string> = {
  'human-review': 'a person has to review it',
  human_review: 'a person has to review it',
  criterion_manual: 'a check only a person can make',
  criterion_failed: 'a check failed',
  file_present: 'a file the task needs is not there',
  file_absent: 'a file that should be gone is still there',
  test_failed: 'a test failed',
  needs_human: 'it waits for a person to decide',
};

const LEVELS = new Set(['L3', 'L4', 'L5']);

/** A verdict line as a sentence, or null when it holds nothing this build knows how to say better. */
export function plainVerdict(raw: string): string | null {
  const text = raw.trim();
  if (text === '') return null;

  const ceiling = /ceiling\s+(L\d)\b(?:\s*\(([^)]*)\))?/i.exec(text);
  if (ceiling && LEVELS.has(ceiling[1].toUpperCase())) {
    const level = ceiling[1].toUpperCase() as keyof typeof LEVEL_MEANING;
    return `This team may go no further than ${LEVEL_MEANING[level].replace(/^L\d: /, `${level}: `)}`;
  }

  const approval = /approval required before\s+(\w+)\s+hop/i.exec(text);
  const asksNobody = /\b(L\d)\s+asks nobody/i.exec(text);
  if (approval) {
    const step = approval[1].charAt(0).toUpperCase() + approval[1].slice(1).toLowerCase();
    return asksNobody
      ? `A person has to approve before the ${step} step, and at ${asksNobody[1].toUpperCase()} nobody is asked, so it cannot go on by itself.`
      : `A person has to approve before the ${step} step.`;
  }

  if (/next steps/i.test(text) && /impossible/i.test(text)) {
    return 'The checks cannot be met as they are written. The next steps say what to change.';
  }

  // "prill: human-review — review: criterion_manual — m1": the verifier's name, its verdict, then what it found.
  const parts = text
    .replace(/^[a-z][\w-]*:\s+(?=[\w-]+\s+—)/i, '')
    .split(/\s+—\s+/)
    .map((p) => p.replace(/^(?:review|check|criterion|stage):\s*/i, '').trim())
    .filter(Boolean);
  const known = parts.filter((p) => p in CODE_WORDS);
  if (known.length > 0) {
    const rest = parts.filter((p) => !(p in CODE_WORDS));
    const said = known.map((p) => CODE_WORDS[p]).join(', and ');
    const first = said.charAt(0).toUpperCase() + said.slice(1);
    return rest.length > 0 ? `${first} (${rest.join(', ')}).` : `${first}.`;
  }
  return null;
}

/** The text a person reads for a line the agent or verifier wrote, and the raw line to show on hover when it differs. */
export function plainLine(raw: string): { text: string; raw: string | undefined } {
  const plain = plainVerdict(raw);
  return plain !== null && plain !== raw ? { text: plain, raw } : { text: raw, raw: undefined };
}

/** Why a proposal could not be read, in one sentence, or null when this build does not know the reason. */
export function plainPlanProblem(raw: string): string | null {
  if (/is not the ref of an earlier row/.test(raw)) return 'One names a parent that is not an earlier row in the file.';
  if (/depends_on .* is not the ref of another row/.test(raw)) return 'One depends on a row that is not in the file.';
  if (/two rows have this ref/.test(raw)) return 'Two rows have the same name.';
  if (/the row has no title/.test(raw)) return 'A row has no title.';
  if (/has no ref/.test(raw)) return 'A row has no name to refer to it by.';
  return null;
}
