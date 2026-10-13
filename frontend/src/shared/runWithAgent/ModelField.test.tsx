import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ModelField from './ModelField';
import type { HarnessCapability } from '../execution/types';

afterEach(() => {
  document.body.innerHTML = '';
});

const DOCS = { models: 'https://example.test/models', sign_in: 'https://example.test/sign-in' };

const report = (extra: Partial<HarnessCapability>): Partial<HarnessCapability> => ({ harness_kind: 'codex', docs: DOCS, ...extra });

function mount(reports: Partial<HarnessCapability>[], value = '', onChange: (v: string) => void = () => {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <ModelField agentLabel="Codex" reports={reports} value={value} onChange={onChange} />, container);
  return container;
}

const options = (c: HTMLElement) =>
  Array.from(c.querySelectorAll<HTMLOptionElement>('select option')).map((o) => o.textContent);

describe('ModelField', () => {
  const cases: [string, Partial<HarnessCapability>, string[], string][] = [
    ['listed, optional', { models: { source: 'listed', ids: ['m-1', 'm-2'], reason: null }, default_model: 'm-1' },
      ["Agent's default (m-1)", 'm-1', 'm-2', 'Another model…'], 'From Codex on this computer.'],
    ['listed, required', { requires_model: true, models: { source: 'listed', ids: ['m-1'], reason: null } },
      ['Choose a model', 'm-1', 'Another model…'], 'From Codex on this computer.'],
    ['suggested, optional', { models: { source: 'suggested', ids: ['s-1'], reason: null } },
      ["Agent's default", 's-1', 'Another model…'],
      "Codex can't list its models here. These are the names its documentation gives; you can type another."],
    ['suggested, required', { requires_model: true, models: { source: 'suggested', ids: ['s-1'], reason: null } },
      ['Choose a model', 's-1', 'Another model…'],
      "Codex can't list its models here. These are the names its documentation gives; you can type another."],
    ['none, optional', { models: { source: 'none', ids: [], reason: 'no list' } },
      ["Agent's default", 'Another model…'], "Codex can't list its models here. Type the model's name as Codex spells it."],
    ['none, required', { requires_model: true, models: { source: 'none', ids: [], reason: 'no list' } },
      ['Choose a model', 'Another model…'], "Codex can't list its models here. Type the model's name as Codex spells it."],
    ['an older runner, no models', {}, ["Agent's default", 'Another model…'],
      "Codex can't list its models here. Type the model's name as Codex spells it."],
  ];

  it.each(cases)('%s: options, line and link', (_name, extra, expectedOptions, line) => {
    const c = mount([report(extra)]);
    expect(options(c)).toEqual(expectedOptions);
    expect(c.querySelector('[data-testid="model-line"]')?.textContent).toBe(`${line} How Codex names models`);
    expect(c.querySelector<HTMLAnchorElement>('[data-testid="model-line"] a')?.href).toBe(DOCS.models);
  });

  it('shows no link when the runner gives no documentation page', () => {
    const c = mount([{ harness_kind: 'codex' }]);
    expect(c.querySelector('[data-testid="model-line"] a')).toBeNull();
  });

  it('merges runners: the ids are the union and a listed source beats a suggested one', () => {
    const c = mount([
      report({ models: { source: 'suggested', ids: ['a', 'b'], reason: null } }),
      report({ models: { source: 'listed', ids: ['b', 'c'], reason: null } }),
    ]);
    expect(options(c)).toEqual(["Agent's default", 'a', 'b', 'c', 'Another model…']);
    expect(c.querySelector('[data-testid="model-line"]')?.textContent).toContain('From Codex on this computer.');
  });

  it('Another model… opens a text field, and a typed name goes out untouched', () => {
    const seen: string[] = [];
    const c = mount([report({})], '', (v) => seen.push(v));
    expect(c.querySelector('input')).toBeNull();
    const select = c.querySelector('select')!;
    select.value = '__other__';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    const input = c.querySelector('input')!;
    input.value = 'provider/Odd Name:1';
    input.dispatchEvent(new Event('change', { bubbles: true }));
    expect(seen).toEqual(['', 'provider/Odd Name:1']);
  });
});
