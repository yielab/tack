/**
 * Enter and Space for an element that acts as a button but cannot be one
 * (a draggable calendar chip or timeline bar). Pair it with role="button"
 * and tabindex={0}.
 */
export function onActivateKey(activate: () => void) {
  return (e: KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activate();
    }
  };
}
