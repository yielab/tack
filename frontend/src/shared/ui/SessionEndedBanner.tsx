import { Show, type Component } from 'solid-js';
import Button from './Button';
import { sessionEnded } from '../api/client';

/** Says once, above every screen, that the session ended: nothing on the page refreshes until it is
 *  loaded again, and loading it is what signs the browser in again. */
const SessionEndedBanner: Component = () => (
  <Show when={sessionEnded()}>
    <div
      role="alert"
      data-testid="session-ended"
      class="flex flex-wrap items-center gap-x-3 gap-y-1.5 px-4 py-2 text-sm"
      style={{ 'border-bottom': '1px solid var(--color-border-light)', color: 'var(--color-danger-600)' }}
    >
      <p>Your session ended, so this page no longer updates. Reload the page to sign in again.</p>
      <Button size="sm" variant="secondary" onClick={() => window.location.reload()}>
        Reload the page
      </Button>
    </div>
  </Show>
);

export default SessionEndedBanner;
