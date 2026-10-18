/** The top-level pages, in the order `routes.tsx` lists them. The document
 *  title and the breadcrumb both read the label from here. */
export const PAGES = [
  { path: '/projects', label: 'Projects' },
  { path: '/templates', label: 'Templates' },
  { path: '/agents', label: 'Agents' },
  { path: '/settings', label: 'Settings' },
] as const;

/** The top-level page a path belongs to; a path not in the table is Projects. */
export function pageOf(pathname: string): (typeof PAGES)[number] {
  return PAGES.find((p) => pathname === p.path || pathname.startsWith(`${p.path}/`)) ?? PAGES[0];
}
