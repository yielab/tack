import { createSignal } from 'solid-js';
import { decisionTokenStore } from '../execution';

// The decision token is asked once for the browser session, wherever the person first meets a
// question, and every place that asks for it reads it from here. The value itself stays in the
// execution module's session-scoped store; this only makes "was one given yet" reactive.

const [revision, bump] = createSignal(0);

/** The token given in this browser session, or null when none was. Re-read whenever it is saved. */
export function sessionDecisionToken(): string | null {
  revision();
  return decisionTokenStore.get();
}

/** Keep the token for this session (an empty one forgets it). Never written anywhere longer-lived. */
export function saveSessionDecisionToken(token: string): void {
  decisionTokenStore.set(token.trim() || null);
  bump((n) => n + 1);
}
