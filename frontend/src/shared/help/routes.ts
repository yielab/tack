/** The book's base URL. The app carries no other docs link; this is the one place to change it. */
export const BOOK_BASE = 'https://yielab.github.io/tack/';

export const BOOK = {
  agents: `${BOOK_BASE}user-guide/running-agents.html#agents-on-this-computer`,
  automation: `${BOOK_BASE}user-guide/automation.html#automation`,
  runDialog: `${BOOK_BASE}user-guide/running-agents.html#the-run-with-agent-dialog`,
  workspace: `${BOOK_BASE}user-guide/automation.html#where-the-agent-works`,
  items: `${BOOK_BASE}user-guide/items.html`,
  importFiles: `${BOOK_BASE}user-guide/import-export.html#the-project-file`,
};

/** The book page for where the user is. `dialogOpen` (the Run dialog) wins over the route. */
export function bookUrlFor(pathname: string, query: { tab?: string; item?: string }, dialogOpen: boolean): string {
  if (dialogOpen) return BOOK.runDialog;
  if (pathname === '/agents') return BOOK.agents;
  if (/^\/projects\/[^/]+\/settings$/.test(pathname) && query.tab === 'automation') return BOOK.automation;
  if (query.item) return BOOK.items;
  return BOOK_BASE;
}
