/** The colour of a status's dot, by the category the workflow gives it. The board's column heading and
 *  the item's status label use the same one; the name beside it always says which status it is. */
export function statusDotColor(category: string | undefined): string {
  switch (category) {
    case 'in_progress': return 'var(--color-primary-600)';
    case 'done': return 'var(--color-success-600)';
    default: return 'var(--color-accent2)';
  }
}
