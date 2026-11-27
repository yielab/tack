import { type Component, For, Show } from 'solid-js';
import { Button, HelpHint, SectionHeading } from '../../shared/ui';
import ItemMenu from '../../shared/ui/ItemMenu';
import { HELP } from '../../shared/help/texts';
import { blankCriterion, controlClass, controlStyle } from './briefRows';
import { FolderMark } from './FolderMark';
import { IconCheck } from './sectionIcons';
import type { BriefEditor } from './useBrief';
import type { FolderMarks } from './useFolderMarks';

/**
 * What makes the task done: the task's own sentence, when it has one (written in the project folder,
 * an import or the API), then what a person checks, with how many are ticked. A task with neither says
 * when the project's definition of done stands in for it, since the run and the readiness use it then.
 * Each tick and each edit saves.
 */
const DoneWhenSection: Component<{ brief: BriefEditor; marks: FolderMarks; projectDone?: string | null }> = (props) => {
  const b = () => props.brief;
  const sentence = () => b().done();
  const fromProject = () => (sentence() === null && b().listed() === 0 ? props.projectDone?.trim() || null : null);
  const share = () => (b().listed() === 0 ? 0 : Math.round((b().ticked() / b().listed()) * 100));
  return (
    <section aria-labelledby="item-done-when" class="space-y-4">
      <SectionHeading
        id="item-done-when"
        icon={<IconCheck />}
        tone="done"
        actions={
          <>
            <HelpHint label="Done when" help={HELP.acceptanceCriteria} />
            <Show when={b().listed() > 0}>
              <span class="text-small font-semibold" aria-live="polite" style={{ color: 'var(--color-text-secondary)' }}>
                {b().ticked()} of {b().listed()}
              </span>
            </Show>
          </>
        }
      >
        Done when
      </SectionHeading>
      <div class="space-y-3">
        <Show when={sentence() !== null}>
          <textarea
            aria-label="Done when, in a sentence"
            data-testid="done-sentence"
            rows={2}
            value={sentence() ?? ''}
            onInput={(e) => b().setDone(e.currentTarget.value)}
            onFocusOut={() => void b().save()}
            class={`${controlClass} resize-none`}
            style={{ ...controlStyle, 'font-size': 'var(--text-body)' }}
          />
        </Show>
        <Show when={fromProject()}>
          {(text) => (
            <p data-testid="done-from-project" class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
              The project's definition of done applies: <span style={{ color: 'var(--color-text-primary)' }}>{text()}</span>
            </p>
          )}
        </Show>
        <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
          A person ticks these, one line per thing checked before calling this done.{' '}
          <FolderMark marks={props.marks} fields={['brief']} />
        </p>
        <Show when={b().listed() > 0}>
          <div
            role="img"
            aria-label={`${b().ticked()} of ${b().listed()} checks ticked`}
            class="h-1.5 w-full rounded-full"
            style={{ 'background-color': 'var(--color-bg-subtle)' }}
          >
            <div class="h-1.5 rounded-full" style={{ width: `${share()}%`, 'background-color': 'var(--color-success-600)' }} />
          </div>
        </Show>
        <Show when={b().errors().acceptance}>
          <p role="alert" class="text-small" style={{ color: 'var(--color-danger-700)' }}>
            {b().errors().acceptance}
          </p>
        </Show>
        <Show when={b().checks.length > 0}>
          <ul class="rounded-md border" style={{ 'border-color': 'var(--color-border-light)' }}>
            <For each={b().checks}>
              {(c, i) => (
                <li
                  class="flex items-center gap-3 border-b px-4 py-2.5 last:border-b-0"
                  style={{ 'border-color': 'var(--color-border-subtle)' }}
                  data-testid="check"
                >
                  <input
                    type="checkbox"
                    class="h-5 w-5 shrink-0"
                    style={{ 'accent-color': 'var(--color-success-600)' }}
                    aria-label={c.text.trim() ? `Done: ${c.text}` : 'Done'}
                    disabled={c.text.trim() === '' || b().saving()}
                    checked={c.checked}
                    onChange={(e) => {
                      b().setChecks(i(), 'checked', e.currentTarget.checked);
                      void b().save();
                    }}
                  />
                  <input
                    aria-label="Acceptance criterion"
                    value={c.text}
                    onInput={(e) => b().setChecks(i(), 'text', e.currentTarget.value)}
                    onFocusOut={() => void b().save()}
                    class={controlClass}
                    style={{ ...controlStyle, 'border-color': 'transparent', 'font-size': 'var(--text-body)' }}
                  />
                  <ItemMenu
                    buttonLabel={`Actions for check ${i() + 1}`}
                    menuLabel="Check actions"
                    entries={[
                      ...(i() > 0 ? [{ label: 'Move up', onSelect: () => { b().move(i(), -1); void b().save(); } }] : []),
                      ...(i() < b().checks.length - 1 ? [{ label: 'Move down', onSelect: () => { b().move(i(), 1); void b().save(); } }] : []),
                      {
                        label: 'Remove',
                        onSelect: () => {
                          b().setChecks((rows) => rows.filter((_, j) => j !== i()));
                          void b().save();
                        },
                      },
                    ]}
                  />
                </li>
              )}
            </For>
          </ul>
        </Show>
        <Button
          variant="ghost"
          size="sm"
          type="button"
          onClick={() => b().setChecks((rows) => [...rows, blankCriterion('manual', b().freshId())])}
        >
          Add check
        </Button>
      </div>
    </section>
  );
};

export default DoneWhenSection;
