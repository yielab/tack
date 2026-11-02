import { ApiError } from '../../shared/api';
import type { Thread, ThreadMessage, Turn } from '../../shared/api/threads';

/** Who wrote a message, in words: the person, the thread's profile (its reply), or an agent through a credential. */
export function writerName(principal: string, profileName: string): string {
  if (principal.startsWith('operator:')) return 'You';
  if (principal.startsWith('thread:')) return profileName;
  if (principal.startsWith('agent:')) return 'An agent';
  return principal;
}

/** True for a message the thread's profile wrote (a reply, or the notice that a turn failed or could not start). */
export const isReply = (m: ThreadMessage): boolean => m.principal.startsWith('thread:');

export interface Entry {
  message: ThreadMessage;
  /** The turn this message started, when it started one. */
  turn: Turn | null;
  /** For a message that is the reason a turn failed: that turn. */
  failedTurn: Turn | null;
}

/**
 * The messages in order, each with the turn it started, and the first reply after a failed turn marked as
 * that turn's reason (the server writes "This turn failed: ..." as the reply).
 */
export function entries(messages: readonly ThreadMessage[], turns: readonly Turn[]): Entry[] {
  const byMessage = new Map(turns.map((t) => [t.message, t]));
  const out: Entry[] = [];
  let waitingForReason: Turn | null = null;
  for (const message of messages) {
    const turn = byMessage.get(message.id) ?? null;
    let failedTurn: Turn | null = null;
    if (isReply(message)) {
      failedTurn = waitingForReason;
      waitingForReason = null;
    } else {
      waitingForReason = turn && turn.state === 'failed' ? turn : null;
    }
    out.push({ message, turn, failedTurn });
  }
  return out;
}

export interface ComposerState {
  disabled: boolean;
  /** Why it is disabled, in plain words; null when it is not. */
  reason: string | null;
}

/** Whether the person may write now, and if not, why. One turn runs at a time. */
export function composerState(thread: Thread, turns: readonly Turn[]): ComposerState {
  if (thread.status === 'closed') {
    const why = thread.closed_reason ? ` ${capitalize(thread.closed_reason)}${/[.!?]$/.test(thread.closed_reason) ? '' : '.'}` : '';
    return { disabled: true, reason: `This thread is closed.${why} Start a new one to go on.` };
  }
  if (turns.some((t) => t.state === 'pending')) {
    return { disabled: true, reason: 'The agent is still working on your last message. You can write again when it has answered.' };
  }
  return { disabled: false, reason: null };
}

export function capitalize(text: string): string {
  return text === '' ? text : text[0].toUpperCase() + text.slice(1);
}

/** What a refused or failed send says: a 409 is the thread's own reason, anything else its message. */
export function sendProblem(error: unknown): string {
  if (error instanceof ApiError && error.status === 409) {
    const why = capitalize(error.message.trim());
    return `The message was not sent. ${/[.!?]$/.test(why) ? why : `${why}.`}`;
  }
  if (error instanceof ApiError && error.status === 403) return 'You may not write in this thread.';
  return error instanceof Error && error.message ? `The message was not sent. ${error.message}` : 'The message was not sent.';
}
