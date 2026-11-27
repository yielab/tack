import { createEffect, createResource, createSignal } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { api, ApiError } from '../../shared/api';
import type { BriefRisk, ItemBrief } from '../../shared/api/briefs';
import { toast } from '../../shared/ui/toast';
import {
  constraintFromRow,
  constraintToRow,
  fromRow,
  sortErrors,
  toRow,
  type ConstraintRow,
  type CriterionRow,
} from './briefRows';

/**
 * An item's brief as the screen edits it. The brief is one document on the server, so the Done when
 * list (the person's checks) and For the agent (the machine's checks, constraints, risk) are edited
 * apart here and saved together. `onSaved` runs after a save, so the screen reads the readiness the
 * server works out from the brief again.
 */
export function useBrief(itemId: () => string, onSaved?: () => void) {
  const [brief] = createResource(itemId, async (id): Promise<ItemBrief | null> => {
    try {
      return await api.briefs.get(id);
    } catch (err) {
      if (err instanceof ApiError && err.status === 404) return null;
      throw err;
    }
  });

  const [checks, setChecks] = createStore<CriterionRow[]>([]);
  const [criteria, setCriteria] = createStore<CriterionRow[]>([]);
  const [constraints, setConstraints] = createStore<ConstraintRow[]>([]);
  const [done, setDone] = createSignal<string | null>(null);
  const [risk, setRisk] = createSignal<BriefRisk | ''>('');
  const [errors, setErrors] = createSignal<{ acceptance?: string; constraints?: string; other?: string }>({});
  const [saving, setSaving] = createSignal(false);
  let nextId = 1;

  const load = (b: ItemBrief) => {
    const rows = b.acceptance.map(toRow);
    setChecks(rows.filter((r) => r.kind === 'manual'));
    setCriteria(rows.filter((r) => r.kind !== 'manual'));
    setConstraints(b.constraints.map(constraintToRow));
    setDone(b.definition_of_done ?? null);
    setRisk(b.risk ?? '');
  };

  createEffect(() => {
    const b = brief();
    if (b?.acceptance) load(b);
  });

  const freshId = () => {
    let id: string;
    do id = `m${nextId++}`;
    while (checks.some((c) => c.id === id) || criteria.some((c) => c.id === id));
    return id;
  };

  const patchCriterion = (i: number, patch: Partial<CriterionRow>) =>
    setCriteria(produce((rows) => Object.assign(rows[i], patch)));

  const move = (i: number, by: number) =>
    setChecks(
      produce((rows) => {
        const j = i + by;
        if (j < 0 || j >= rows.length) return;
        [rows[i], rows[j]] = [rows[j], rows[i]];
      }),
    );

  const save = async () => {
    setSaving(true);
    setErrors({});
    try {
      const manual = checks
        .filter((c) => c.text.trim() !== '')
        .map((c) => fromRow({ ...c, title: c.text.slice(0, 60) }));
      const saved = await api.briefs.put(itemId(), {
        acceptance: [...manual, ...criteria.map(fromRow)],
        constraints: constraints.map(constraintFromRow),
        definition_of_done: done()?.trim() || null,
        risk: risk() || null,
      });
      load(saved);
      onSaved?.();
    } catch (err) {
      if (err instanceof ApiError && err.status === 400) setErrors(sortErrors(err.message));
      else toast.error(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  const ticked = () => checks.filter((c) => c.checked && c.text.trim() !== '').length;
  const listed = () => checks.filter((c) => c.text.trim() !== '').length;

  return {
    checks,
    setChecks,
    criteria,
    setCriteria,
    constraints,
    setConstraints,
    risk,
    setRisk,
    done,
    setDone,
    errors,
    saving,
    save,
    freshId,
    patchCriterion,
    move,
    ticked,
    listed,
  };
}

export type BriefEditor = ReturnType<typeof useBrief>;
