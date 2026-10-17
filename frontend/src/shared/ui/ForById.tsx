import { createMemo, For, type JSX } from 'solid-js';

const sameIds = (a: string[], b: string[]) => a.length === b.length && a.every((id, i) => id === b[i]);

/**
 * `<For>` keyed by `id` instead of by object. A refetch hands back a new
 * object for every row that changed; `<For>` would rebuild that row, and with
 * it whatever the row holds open — a "Run with agent" dialog, a focused
 * button. Here a row lives as long as its id does, and `row()` always reads
 * the newest object (or, in the instant before a removed row is dropped, the
 * last one it had).
 */
export function ForById<T extends { id: string }>(props: {
  each: readonly T[] | undefined;
  children: (row: () => T) => JSX.Element;
}): JSX.Element {
  const ids = createMemo(() => (props.each ?? []).map((row) => row.id), undefined, { equals: sameIds });
  const byId = createMemo(() => new Map((props.each ?? []).map((row) => [row.id, row])));
  return (
    <For each={ids()}>
      {(id) => {
        let last: T | undefined;
        const row = () => (last = byId().get(id) ?? last)!;
        return props.children(row);
      }}
    </For>
  );
}
