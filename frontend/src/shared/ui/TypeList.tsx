import { type Component, For, Show } from 'solid-js';
import clsx from 'clsx';
import type { ItemTypeDef } from '../types';
import { typeTone } from './TypeTile';

/**
 * The types a template or a project type starts with, as small tiles in the order the project keeps
 * them. `TypeTile` reads a type from the open project's catalog; a template has no project yet, so
 * this draws from the types it carries, with the same rule: a type that holds work has square
 * corners, work has round ones.
 */
const TypeList: Component<{ types: readonly ItemTypeDef[] | null | undefined; class?: string }> = (props) => (
  <Show when={(props.types?.length ?? 0) > 0}>
    <ul aria-label="Types" data-testid="type-list" class={clsx('m-0 flex list-none flex-wrap gap-1 p-0', props.class)}>
      <For each={props.types}>
        {(t) => (
          <li
            data-holds-work={t.level === 'group' ? 'true' : 'false'}
            title={t.level === 'group' ? `${t.name} holds work` : undefined}
            class="whitespace-nowrap px-2 py-px text-micro font-bold"
            style={{
              'background-color': typeTone(t.color).bg,
              color: typeTone(t.color).fg,
              'border-radius': t.level === 'group' ? 'var(--radius-sm)' : 'var(--radius-pill)',
            }}
          >
            {t.name}
            <Show when={t.level === 'group'}>
              <span class="sr-only"> (holds work)</span>
            </Show>
          </li>
        )}
      </For>
    </ul>
  </Show>
);

export default TypeList;
