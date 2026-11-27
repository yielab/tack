import { type Component, For, Show, createEffect, createSignal, on } from 'solid-js';
import { api, ApiError } from '../../../../shared/api';
import type { Change, FolderApplied, FolderPlan, PlanSource, Prefer, Verb } from '../../../../shared/api/folder';
import { BOOK } from '../../../../shared/help/routes';
import { Badge, Button, Modal } from '../../../../shared/ui';
import type { BadgeTone } from '../../../../shared/ui/Badge';
import { VERB_LABEL, bySection, formatValue, revisionLine, sectionTitle } from './planText';

const VERB_TONE: Record<Verb, BadgeTone> = { create: 'success', update: 'info', remove: 'danger', keep: 'neutral' };

const PREFER_OPTIONS: { value: Prefer | undefined; label: string }[] = [
  { value: undefined, label: 'Ask' },
  { value: 'file', label: 'Prefer the folder' },
  { value: 'board', label: 'Prefer the board' },
];

const H = 'text-base font-semibold';
const MUTED = { color: 'var(--color-text-secondary)' };
const PRIMARY = { color: 'var(--color-text-primary)' };

const ChangeRow: Component<{ change: Change }> = (props) => (
  <li data-testid="plan-change" data-verb={props.change.verb} class="flex flex-col gap-1 py-2.5">
    <div class="flex flex-wrap items-center gap-2">
      <Badge tone={VERB_TONE[props.change.verb]}>{VERB_LABEL[props.change.verb]}</Badge>
      <span class="font-semibold" style={PRIMARY}>{props.change.name}</span>
      <span class="font-mono text-micro" style={{ color: 'var(--color-text-tertiary)' }}>{props.change.key}</span>
    </div>
    <Show when={props.change.reason}>
      <p class="text-sm" style={MUTED}>{props.change.reason}</p>
    </Show>
    <Show when={props.change.fields.length > 0}>
      <ul class="flex flex-col gap-0.5 pl-1 text-sm" style={MUTED}>
        <For each={props.change.fields}>
          {(field) => (
            <li>
              <span class="font-mono text-small" style={PRIMARY}>{field.path}</span>: {formatValue(field.from)} → {formatValue(field.to)}
            </li>
          )}
        </For>
      </ul>
    </Show>
  </li>
);

/**
 * What applying a folder (or going back to a revision) does, before it is done. Every change by
 * section, the conflicts with their three values, and the removals with what they delete; Apply
 * sends the hash of the plan shown, so a board changed since then is refused and planned again.
 */
const PlanDialog: Component<{
  projectId: string;
  source: PlanSource;
  title: string;
  onClose: () => void;
  onApplied: (applied: FolderApplied) => void;
}> = (props) => {
  const [prefer, setPrefer] = createSignal<Prefer | undefined>(undefined);
  const [plan, setPlan] = createSignal<FolderPlan | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [stale, setStale] = createSignal<string | null>(null);
  const [readRemovals, setReadRemovals] = createSignal(false);
  const [busy, setBusy] = createSignal(false);

  const preview = async () => {
    setBusy(true);
    setError(null);
    setStale(null);
    setReadRemovals(false);
    try {
      setPlan(await api.folder.plan(props.projectId, props.source, prefer()));
    } catch (err) {
      setPlan(null);
      setError(err instanceof Error ? err.message : 'The folder was refused');
    } finally {
      setBusy(false);
    }
  };
  createEffect(on(prefer, () => void preview()));

  const apply = async () => {
    const current = plan();
    if (!current) return;
    setBusy(true);
    try {
      props.onApplied(await api.folder.apply(props.projectId, props.source, current.plan_hash, prefer()));
    } catch (err) {
      const message = err instanceof Error ? err.message : 'The apply failed';
      if (err instanceof ApiError && err.status === 409) setStale(message);
      else setError(message);
    } finally {
      setBusy(false);
    }
  };

  // A removal is listed once, under Removed, with its cost; the other rows go under their section.
  const listed = () => {
    const current = plan();
    if (!current) return [];
    const removed = new Set(current.removals.map((r) => `${r.section}/${r.key}`));
    return bySection(current.changes.filter((c) => !(c.verb === 'remove' && removed.has(`${c.section}/${c.key}`))));
  };
  const nothing = () => {
    const current = plan();
    return current !== null && current.changes.length === 0 && current.conflicts.length === 0 && current.removals.length === 0;
  };
  const canApply = () => {
    const current = plan();
    return current !== null && current.applies && !nothing() && (current.removals.length === 0 || readRemovals()) && !stale() && !busy();
  };

  return (
    <Modal isOpen onClose={props.onClose} title={props.title} size="lg" help={BOOK.projectFolder}>
      <div class="flex flex-col gap-5 text-sm" style={MUTED}>
        <Show when={error()}>
          {(message) => <p role="alert" class="whitespace-pre-wrap" style={{ color: 'var(--color-danger-600)' }}>{message()}</p>}
        </Show>

        <Show when={plan()}>
          {(current) => (
            <>
              <p>
                {current().base ? `Compared with the board and ${revisionLine(current().base!)}.` : 'No revision yet: this plan removes nothing.'}{' '}
                Status, assignee, priority, sprint and due date are the board's: a file never changes them for an item that exists.
              </p>

              <Show when={nothing()}>
                <p style={PRIMARY}>The folder and the board already agree. Nothing to apply.</p>
              </Show>

              <For each={listed()}>
                {(group) => (
                  <section aria-label={sectionTitle(group.section)} data-testid="plan-section">
                    <h3 class={H} style={PRIMARY}>{sectionTitle(group.section)}</h3>
                    <ul class="divide-y" style={{ 'border-color': 'var(--color-border-light)' }}>
                      <For each={group.changes}>{(change) => <ChangeRow change={change} />}</For>
                    </ul>
                  </section>
                )}
              </For>

              <Show when={current().conflicts.length > 0 || prefer() !== undefined}>
                <section aria-labelledby="plan-conflicts" data-testid="plan-conflicts" class="flex flex-col gap-3 rounded-lg p-4" style={{ background: 'var(--color-warning-100)' }}>
                  <h3 id="plan-conflicts" class={H} style={PRIMARY}>
                    Conflicts · {current().conflicts.length}
                  </h3>
                  <p>The folder and the board both changed these since the last revision.</p>
                  <ul class="flex flex-col gap-3">
                    <For each={current().conflicts}>
                      {(conflict) => (
                        <li data-testid="plan-conflict" class="flex flex-col gap-1">
                          <span style={PRIMARY}>
                            <span class="font-semibold">{conflict.name}</span> · <span class="font-mono text-small">{conflict.path}</span>
                          </span>
                          <span>Last revision: {formatValue(conflict.base)}</span>
                          <span>The board's: {formatValue(conflict.board)}</span>
                          <span>The folder's: {formatValue(conflict.file)}</span>
                        </li>
                      )}
                    </For>
                  </ul>
                  <fieldset class="flex flex-wrap items-center gap-x-4 gap-y-2">
                    <legend class="mb-1 font-semibold" style={PRIMARY}>When both changed</legend>
                    <For each={PREFER_OPTIONS}>
                      {(option) => (
                        <label class="inline-flex items-center gap-1.5" style={PRIMARY}>
                          <input
                            type="radio"
                            name="plan-prefer"
                            checked={prefer() === option.value}
                            disabled={busy()}
                            onChange={() => setPrefer(option.value)}
                          />
                          {option.label}
                        </label>
                      )}
                    </For>
                    <label class="inline-flex items-center gap-1.5" title="Not built yet: one side is taken for every conflict.">
                      <input type="radio" name="plan-prefer" disabled />
                      Choose per conflict
                      <span class="text-caption">(not built yet: one side for every conflict)</span>
                    </label>
                  </fieldset>
                </section>
              </Show>

              <Show when={current().removals.length > 0}>
                <section aria-labelledby="plan-removed" data-testid="plan-removals" class="flex flex-col gap-2 rounded-lg p-4" style={{ background: 'var(--color-danger-100)' }}>
                  <h3 id="plan-removed" class={H} style={PRIMARY}>Removed · {current().removals.length}</h3>
                  <ul class="flex flex-col gap-2">
                    <For each={current().removals}>
                      {(removal) => (
                        <li data-testid="plan-removal" class="flex flex-col gap-0.5">
                          <span style={PRIMARY}>
                            {sectionTitle(removal.section)}: <span class="font-semibold">{removal.name}</span>{' '}
                            <span class="font-mono text-micro" style={{ color: 'var(--color-text-tertiary)' }}>{removal.key}</span>
                          </span>
                          <For each={removal.cost}>{(line) => <span>{line}</span>}</For>
                        </li>
                      )}
                    </For>
                  </ul>
                  <label class="mt-1 inline-flex items-center gap-2 font-semibold" style={PRIMARY}>
                    <input type="checkbox" checked={readRemovals()} onChange={(e) => setReadRemovals(e.currentTarget.checked)} />
                    I have read what these removals delete
                  </label>
                </section>
              </Show>

              <Show when={current().warnings.length > 0}>
                <section aria-label="Warnings">
                  <h3 class={H} style={PRIMARY}>Read differently than written</h3>
                  <ul class="list-disc pl-5">
                    <For each={current().warnings}>{(w) => <li>{w}</li>}</For>
                  </ul>
                </section>
              </Show>
            </>
          )}
        </Show>

        <Show when={stale()}>
          {(message) => <p role="alert" style={PRIMARY}>{message()}</p>}
        </Show>

        <div class="flex flex-wrap items-center justify-end gap-2">
          <Show when={plan() && !plan()!.applies && !stale()}>
            <span class="mr-auto">Choose a side for the conflicts to apply.</span>
          </Show>
          <Button variant="secondary" onClick={props.onClose} disabled={busy()}>
            {nothing() || error() ? 'Close' : 'Cancel'}
          </Button>
          <Show
            when={!stale()}
            fallback={<Button onClick={() => void preview()} disabled={busy()}>Preview again</Button>}
          >
            <Show when={!nothing() && !error()}>
              <Button onClick={() => void apply()} disabled={!canApply()}>Apply</Button>
            </Show>
          </Show>
        </div>
      </div>
    </Modal>
  );
};

export default PlanDialog;
