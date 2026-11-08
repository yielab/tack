import { type Component, Show, createSignal } from 'solid-js';
import { Button } from '../../shared/ui';
import PlanPanel, { type PlanEntry } from '../../shared/runWithAgent/PlanPanel';

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isIndex = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0;

/**
 * Reads a tack-plan.json against docs/contracts/plan-v1/schema.json and the index rules in its README:
 * the first entry that does not fit is named, with why, in the words the person sees.
 */
function checkPlanFile(text: string): { entries: PlanEntry[] } | { error: string } {
  let doc: unknown;
  try {
    doc = JSON.parse(text);
  } catch (e) {
    return { error: `The file is not JSON: ${e instanceof Error ? e.message : 'it does not parse'}.` };
  }
  if (!isObject(doc) || doc.v !== '1') return { error: 'The file must be a plan with "v" set to "1".' };
  if (!Array.isArray(doc.subtasks)) return { error: 'The file needs a "subtasks" list.' };
  const list = doc.subtasks as unknown[];
  if (list.length === 0) return { error: 'The file has no entries.' };

  const entries: PlanEntry[] = [];
  for (let i = 0; i < list.length; i++) {
    const raw = list[i];
    const title = isObject(raw) && typeof raw.title === 'string' ? raw.title : '';
    const where = title ? `Entry ${i + 1} ("${title}")` : `Entry ${i + 1}`;
    const fail = (why: string) => ({ error: `${where}: ${why}` });
    if (!isObject(raw)) return fail('it is not an object.');
    if (typeof raw.title !== 'string' || raw.title.trim() === '') return fail('"title" must be text, and not empty.');
    if (typeof raw.description !== 'string') return fail('"description" must be text.');
    if (raw.acceptance !== undefined && (!Array.isArray(raw.acceptance) || raw.acceptance.some((a) => typeof a !== 'string'))) {
      return fail('"acceptance" must be a list of sentences.');
    }
    if (raw.estimate !== undefined && !['S', 'M', 'L'].includes(raw.estimate as string)) {
      return fail(`"estimate" must be S, M or L, not ${JSON.stringify(raw.estimate)}.`);
    }
    for (const key of ['type', 'role', 'team'] as const) {
      if (raw[key] !== undefined && typeof raw[key] !== 'string') return fail(`"${key}" must be text.`);
    }
    let depends: number[] = [];
    if (raw.depends_on !== undefined) {
      if (!Array.isArray(raw.depends_on) || !raw.depends_on.every(isIndex)) {
        return fail('"depends_on" must be a list of entry numbers.');
      }
      depends = raw.depends_on as number[];
      for (const d of depends) {
        if (d >= list.length) return fail(`"depends_on" names entry ${d + 1}, and the file has ${list.length} entries.`);
        if (d === i) return fail('"depends_on" names the entry itself.');
      }
    }
    let parent: number | undefined;
    if (raw.parent !== undefined) {
      if (!isIndex(raw.parent) || raw.parent >= i) {
        return fail(`"parent" must name an earlier entry, not ${JSON.stringify(raw.parent)}.`);
      }
      parent = raw.parent;
    }
    entries.push({
      title: raw.title,
      description: raw.description,
      acceptance: (raw.acceptance as string[] | undefined) ?? [],
      depends_on: depends,
      estimate: raw.estimate as string | undefined,
      type: raw.type as string | undefined,
      role: raw.role as string | undefined,
      team: raw.team as string | undefined,
      parent,
    });
  }
  return { entries };
}

export interface ProposeFromFileProps {
  itemId: string;
  /** With it, the section has a Close button. */
  onClose?: () => void;
}

/** "Propose items from a file", opened from the item's "⋯" menu: the person picks a tack-plan.json; the same panel opens. */
const ProposeFromFile: Component<ProposeFromFileProps> = (props) => {
  const [plan, setPlan] = createSignal<PlanEntry[] | null>(null);
  const [problem, setProblem] = createSignal<string | null>(null);
  const [dragging, setDragging] = createSignal(false);
  const read = async (file: File) => {
    setPlan(null);
    setProblem(null);
    try {
      const result = checkPlanFile(await file.text());
      if ('error' in result) setProblem(result.error);
      else setPlan(result.entries);
    } catch {
      setProblem('The file could not be read.');
    }
  };
  const pick = (input: HTMLInputElement) => {
    const file = input.files?.[0];
    input.value = '';
    if (file) void read(file);
  };
  const drop = (e: DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const file = e.dataTransfer?.files[0];
    if (file) void read(file);
  };

  return (
    <section class="min-w-0 space-y-2" aria-label="Propose items from a file">
      <Show when={props.onClose}>
        <div class="flex justify-end">
          <Button size="sm" variant="ghost" onClick={() => props.onClose?.()}>
            Close
          </Button>
        </div>
      </Show>
      {/* The drop zone is the file button's box: a file dropped anywhere in it is read as if it were picked. */}
      <div
        class="rounded-item p-2 text-sm"
        style={{
          border: dragging() ? '1px dashed var(--color-accent-line)' : '1px dashed var(--color-border-light)',
          'background-color': dragging() ? 'var(--color-accent-soft)' : 'transparent',
        }}
        onDragOver={(e) => {
          // Without this the browser keeps the file and the drop never fires.
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDragging(false);
        }}
        onDrop={drop}
      >
        <label class="block text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>Propose items from a file</span>
          <span class="block text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            Choose a tack-plan.json, or drop one here. Nothing is created until you confirm the items below.
          </span>
          <input
            type="file"
            accept=".json,application/json"
            class="mt-1 text-sm"
            onChange={(e) => pick(e.currentTarget)}
          />
        </label>
      </div>
      <Show when={problem()}>
        {(text) => (
          <p class="text-sm" role="alert" style={{ color: 'var(--color-danger-600)' }}>
            {text()}
          </p>
        )}
      </Show>
      <Show when={plan()} keyed>
        {(entries) => <PlanPanel itemId={props.itemId} file={entries} />}
      </Show>
    </section>
  );
};

export default ProposeFromFile;
