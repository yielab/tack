// Keeps Tab and Shift+Tab inside an open dialog. The page behind a modal is
// not inert, so without this the keyboard walks out into controls the user
// cannot see under the overlay.

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), ' +
  'textarea:not([disabled]), summary, [contenteditable="true"], [tabindex]:not([tabindex="-1"])';

function tabbable(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) =>
      el.tabIndex >= 0 &&
      !el.closest('[hidden], [inert]') &&
      (typeof el.checkVisibility !== 'function' || el.checkVisibility()),
  );
}

/**
 * Call from a dialog's keydown listener. Wraps focus at either end of the
 * dialog. When focus sits in another modal dialog (one stacked on top of
 * this), that dialog's own trap owns the key and this one stays out of it.
 */
export function trapFocus(container: HTMLElement | undefined, e: KeyboardEvent): void {
  if (e.key !== 'Tab' || !container) return;
  const active = document.activeElement;
  if (active instanceof Element && !container.contains(active) && active.closest('[aria-modal="true"]')) return;

  const items = tabbable(container);
  if (items.length === 0) {
    e.preventDefault();
    container.focus();
    return;
  }
  const first = items[0];
  const last = items[items.length - 1];
  if (!(active instanceof Element) || !container.contains(active)) {
    e.preventDefault();
    (e.shiftKey ? last : first).focus();
  } else if (e.shiftKey && (active === first || active === container)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && active === last) {
    e.preventDefault();
    first.focus();
  }
}
