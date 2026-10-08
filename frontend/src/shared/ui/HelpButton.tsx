import type { Component } from 'solid-js';
import { useLocation } from '@solidjs/router';
import { bookUrlFor } from '../help/routes';

/** Top-bar button: opens the book page for the current route, or for the Run dialog while it is open. */
const HelpButton: Component = () => {
  const location = useLocation();
  const open = () => {
    const dialogOpen = !!document.querySelector('[role="dialog"][aria-label^="Run with agent"]');
    const url = bookUrlFor(location.pathname, location.query as { tab?: string; item?: string }, dialogOpen);
    window.open(url, '_blank', 'noreferrer');
  };
  return (
    <button
      type="button"
      onClick={open}
      title="Help"
      aria-label="Help"
      style={{
        display: 'flex', 'align-items': 'center', padding: '6px 10px',
        'border-radius': 'var(--radius-pill)', cursor: 'pointer',
        border: '1px solid var(--color-border-light)', background: 'transparent',
        color: 'var(--color-text-secondary)', 'font-size': '13px',
      }}
    >
      ?
    </button>
  );
};

export default HelpButton;
