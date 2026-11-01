import { marked } from 'marked';

const HTML_TAG = /<[a-z][a-z0-9-]*(\s[^>]*)?>/i;

/**
 * The editor stores HTML, but a description that arrived through a project file,
 * `tack import` or MCP is Markdown. A value with no HTML tag is Markdown; the
 * result still goes through `sanitizeRichText` before it is shown.
 */
export function descriptionToHtml(value: string): string {
  if (value.trim() === '' || HTML_TAG.test(value)) return value;
  return marked.parse(value, { async: false, gfm: true, breaks: true });
}
