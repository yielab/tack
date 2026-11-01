/** An id shown as its head and tail, `dec_6174…3d6f`, so a long one keeps its row's width.
 * The full value belongs in the element's `title` and in what Copy copies. */
export function truncateMiddle(value: string, head = 8, tail = 4): string {
  if (value.length <= head + tail + 1) return value;
  return `${value.slice(0, head)}…${value.slice(-tail)}`;
}
