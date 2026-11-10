import { BOOK } from './routes';
import type { FieldHelp } from '../ui/Field';

// Apart from `HELP` so the shell, which asks for this token in "Waiting for you", does not carry every field's help text.
export const DECISION_TOKEN_HELP: FieldHelp = {
  text: 'A second secret, apart from signing in, that your administrator sets up. Needed to answer the agent; never sent when you only look.',
  href: BOOK.decisionToken,
};
