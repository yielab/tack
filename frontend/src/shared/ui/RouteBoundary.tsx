import { ErrorBoundary, type Component, type JSX } from 'solid-js';
import Button from './Button';

/** Catches whatever a screen throws while it renders or loads, and says so in its place instead of a blank page.
 *  "Try again" draws the screen from scratch. One wraps the app and one wraps each route. */
const RouteBoundary: Component<{ children: JSX.Element }> = (props) => (
  <ErrorBoundary
    fallback={(err, reset) => {
      console.error(err);
      return (
        <div role="alert" class="flex flex-col items-start gap-3 px-6 py-14" data-testid="route-error">
          <h1 class="text-[1.75rem] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
            This page could not be shown
          </h1>
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            {err instanceof Error && err.message ? err.message : 'Something went wrong while drawing it.'}
          </p>
          <div class="flex flex-wrap items-center gap-3">
            <Button onClick={reset}>Try again</Button>
            <a href="/projects" class="text-sm underline" style={{ color: 'var(--color-accent-ink)' }}>
              All projects
            </a>
          </div>
        </div>
      );
    }}
  >
    {props.children}
  </ErrorBoundary>
);

export default RouteBoundary;
