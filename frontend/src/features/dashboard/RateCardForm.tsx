import { createResource, createSignal, For, Show } from 'solid-js';
import { request } from '../../shared/api/client';
import { Button, Field } from '../../shared/ui';
import type { components } from '../../shared/api/schema.gen';

type RateCard = components['schemas']['RateCard'];

/** One editable row: a model string and its two prices. */
interface Row {
  id: number;
  model: string;
  input: string;
  output: string;
}

export interface RateCardFormProps {
  /** Called after a successful save, so the page can refetch the cost it sets. */
  onSaved?: () => void;
}

/** The board's rate card: what each model costs per million tokens, plus CI and human time. */
export default function RateCardForm(props: RateCardFormProps) {
  const [card] = createResource(async () => {
    const loaded = await request<RateCard | null>('/settings/rate-card');
    setAsOf(loaded?.as_of ?? '');
    setCiMinute(loaded ? String(loaded.ci_minute) : '');
    setHumanHour(loaded ? String(loaded.human_hour) : '');
    const entries = Object.entries(loaded?.models ?? {});
    setRows(entries.length ? entries.map(([model, price]) => toRow(model, price)) : [blankRow()]);
    return loaded ?? null;
  });

  const [asOf, setAsOf] = createSignal('');
  const [ciMinute, setCiMinute] = createSignal('');
  const [humanHour, setHumanHour] = createSignal('');
  const [rows, setRows] = createSignal<Row[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [message, setMessage] = createSignal<{ tone: 'ok' | 'error'; text: string } | null>(null);

  let nextId = 0;
  function blankRow(): Row {
    return { id: nextId++, model: '', input: '', output: '' };
  }
  function toRow(model: string, price: { input_per_mtok: number; output_per_mtok: number }): Row {
    return { id: nextId++, model, input: String(price.input_per_mtok), output: String(price.output_per_mtok) };
  }

  function patchRow(id: number, patch: Partial<Omit<Row, 'id'>>) {
    setRows((rs) => rs.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  }

  /** The card as the server expects it, or one sentence saying what to fix. */
  function toCard(): RateCard | string {
    if (asOf().trim() === '') return 'Say the date the prices were read.';
    const price = (s: string) => (s.trim() === '' ? NaN : Number(s));
    const ci = price(ciMinute());
    const hh = price(humanHour());
    if (!(ci >= 0) || !(hh >= 0)) return 'CI minute and human hour need a price of zero or more.';

    const models: RateCard['models'] = {};
    for (const r of rows()) {
      const name = r.model.trim();
      if (name === '' && r.input.trim() === '' && r.output.trim() === '') continue;
      const input = price(r.input);
      const output = price(r.output);
      if (name === '' || !(input >= 0) || !(output >= 0)) {
        return 'Each model needs a name and two prices of zero or more.';
      }
      if (name in models) return `${name} is listed twice.`;
      models[name] = { input_per_mtok: input, output_per_mtok: output };
    }
    return { as_of: asOf().trim(), ci_minute: ci, human_hour: hh, models };
  }

  function save(e: SubmitEvent) {
    e.preventDefault();
    const body = toCard();
    if (typeof body === 'string') {
      setMessage({ tone: 'error', text: body });
      return;
    }
    setSaving(true);
    setMessage(null);
    request<RateCard>('/settings/rate-card', { method: 'PUT', body: JSON.stringify(body) })
      .then(() => {
        setMessage({ tone: 'ok', text: `Saved. Prices as of ${body.as_of}.` });
        props.onSaved?.();
      })
      .catch((err: unknown) => {
        setMessage({ tone: 'error', text: err instanceof Error ? err.message : 'The rate card did not save.' });
      })
      .finally(() => setSaving(false));
  }

  return (
    <section class="rounded-card bg-panel px-5 py-[18px] flex flex-col gap-3">
      <div>
        <h2 class="text-lg text-content">Rate card</h2>
        <p class="text-xs text-content-muted">
          Tack prices each run's tokens and human minutes with this card. It applies to the whole board.
        </p>
      </div>

      <Show when={!card.loading} fallback={<p class="text-sm text-content-muted">Loading the rate card…</p>}>
        <Show when={card() === null}>
          <p class="text-sm text-content-muted">No rate card yet. Enter prices below to see cost.</p>
        </Show>

        <form class="flex flex-col gap-3" onSubmit={save}>
          <Field
            label="Prices as of"
            value={asOf()}
            onInput={(e) => setAsOf(e.currentTarget.value)}
            placeholder="2026-10-01"
          />

          <div class="overflow-x-auto">
            <table class="w-full text-sm text-content">
              <thead>
                <tr class="text-left text-xs text-content-muted">
                  <th class="font-normal pb-1 pr-2">Model string</th>
                  <th class="font-normal pb-1 pr-2">Input, USD per million tokens</th>
                  <th class="font-normal pb-1 pr-2">Output, USD per million tokens</th>
                  <th class="font-normal pb-1" />
                </tr>
              </thead>
              <tbody>
                <For each={rows()}>
                  {(row) => (
                    <tr>
                      <td class="pr-2 pb-1">
                        <input
                          class="w-full bg-transparent border-b border-current/20 px-1 py-0.5"
                          aria-label="Model string"
                          value={row.model}
                          onInput={(e) => patchRow(row.id, { model: e.currentTarget.value })}
                        />
                      </td>
                      <td class="pr-2 pb-1">
                        <input
                          class="w-full bg-transparent border-b border-current/20 px-1 py-0.5"
                          type="number"
                          step="any"
                          min="0"
                          aria-label="Input price"
                          value={row.input}
                          onInput={(e) => patchRow(row.id, { input: e.currentTarget.value })}
                        />
                      </td>
                      <td class="pr-2 pb-1">
                        <input
                          class="w-full bg-transparent border-b border-current/20 px-1 py-0.5"
                          type="number"
                          step="any"
                          min="0"
                          aria-label="Output price"
                          value={row.output}
                          onInput={(e) => patchRow(row.id, { output: e.currentTarget.value })}
                        />
                      </td>
                      <td class="pb-1">
                        <Button
                          type="button"
                          variant="secondary"
                          size="sm"
                          aria-label={`Remove ${row.model || 'blank row'}`}
                          onClick={() => setRows((rs) => rs.filter((r) => r.id !== row.id))}
                        >
                          Remove
                        </Button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>

          <div>
            <Button type="button" variant="secondary" size="sm" onClick={() => setRows((rs) => [...rs, blankRow()])}>
              Add a model
            </Button>
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <Field
              label="CI minute, USD"
              type="number"
              step="any"
              min="0"
              value={ciMinute()}
              onInput={(e) => setCiMinute(e.currentTarget.value)}
            />
            <Field
              label="Human hour, USD"
              type="number"
              step="any"
              min="0"
              value={humanHour()}
              onInput={(e) => setHumanHour(e.currentTarget.value)}
            />
          </div>

          <div class="flex items-center gap-3">
            <Button type="submit" size="sm" loading={saving()}>
              Save rate card
            </Button>
            <Show when={message()}>
              {(m) => (
                <p role={m().tone === 'error' ? 'alert' : 'status'} class="text-xs text-content-muted">
                  {m().text}
                </p>
              )}
            </Show>
          </div>
        </form>
      </Show>
      <Show when={card.error}>
        <p role="alert" class="text-xs text-content-muted">
          The rate card could not be loaded: {(card.error as Error).message}
        </p>
      </Show>
    </section>
  );
}
