import { type Component, type Resource, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Fold } from '../../shared/ui';
import { itemHref } from '../../shared/ui/ItemPath';
import type { Dependency, Item } from '../../shared/types';
import DependenciesSection, { classify } from './sections/DependenciesSection';
import FieldsSection from './sections/FieldsSection';
import FilesSection from './sections/FilesSection';
import GithubIssueSection from './sections/GithubIssueSection';
import { FolderMark } from './FolderMark';
import type { FolderMarks } from './useFolderMarks';
import type { ListedItem } from './useItemList';

export interface RelatedPanelProps {
  item: Item;
  /** The project's items, to name the parent. */
  byId: ReadonlyMap<string, ListedItem>;
  dependencies: Resource<Dependency[]>;
  refetchDependencies: () => unknown;
  marks: FolderMarks;
}

/** What the item is tied to: its parent, what blocks it or waits for it, its files, its GitHub issue and its custom fields. Each folds to a line. */
const RelatedPanel: Component<RelatedPanelProps> = (props) => {
  const edges = () => {
    const { dependencies } = props;
    return dependencies.state === 'ready' || dependencies.state === 'refreshing' ? dependencies() : undefined;
  };
  const count = (dir: 'blocks' | 'blockedBy') => edges()?.filter((d) => classify(d, props.item.id).dir === dir).length;
  const dependencySummary = () => {
    const blocks = count('blocks');
    const blockedBy = count('blockedBy');
    if (blocks === undefined || blockedBy === undefined) return undefined;
    if (blocks === 0 && blockedBy === 0) return 'none';
    return [blockedBy > 0 ? `blocked by ${blockedBy}` : null, blocks > 0 ? `blocks ${blocks}` : null].filter(Boolean).join(' · ');
  };
  const parent = () => (props.item.parent_id ? props.byId.get(props.item.parent_id) : undefined);

  return (
    <section aria-label="Related" class="space-y-3">
      <div class="flex items-baseline gap-2">
        <h2 class="text-lead font-semibold" style={{ color: 'var(--color-text-primary)' }}>Related</h2>
        <FolderMark marks={props.marks} fields={['parent', 'blockers', 'roles', 'fields']} />
      </div>
      <div>
        <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>Parent</p>
        <Show
          when={parent()}
          fallback={<p class="text-body" data-testid="related-parent-none" style={{ color: 'var(--color-text-secondary)' }}>None</p>}
        >
          {(p) => (
            <A
              data-testid="related-parent"
              href={itemHref(props.item.project_id, p().id)}
              class="text-body font-medium hover:underline"
              style={{ color: 'var(--color-accent-ink)' }}
            >
              {p().title}
            </A>
          )}
        </Show>
      </div>
      <div class="space-y-1 border-t pt-2" style={{ 'border-color': 'var(--color-border-subtle)' }}>
        <Fold kind="dependencies" title="Depends on" summary={dependencySummary()} lazy>
          <DependenciesSection item={props.item} dependencies={props.dependencies} refetch={props.refetchDependencies} />
        </Fold>
        <Fold kind="files" title="Files" lazy>
          <FilesSection itemId={props.item.id} />
        </Fold>
        <GithubIssueSection item={props.item} />
        <Fold kind="fields" title="Custom fields and roles" lazy>
          <FieldsSection item={props.item} />
        </Fold>
      </div>
    </section>
  );
};

export default RelatedPanel;
