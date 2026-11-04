import { marked } from 'marked';
import type { Component } from 'solid-js';
import { sanitizeReport } from './RichTextEditor';

/** Markdown as a reader sees it: headings, lists, tables and links, not the marks that make them.
 *  The text is an agent's, so the HTML goes through the same cleaning as every rich text. */
export const Markdown: Component<{ text: string; testId?: string }> = (props) => (
  <div
    class="markdown-body"
    data-testid={props.testId}
    innerHTML={sanitizeReport(marked.parse(props.text, { async: false, gfm: true }))}
  />
);

export default Markdown;
