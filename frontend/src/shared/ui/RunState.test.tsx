import { describe, it, expect, afterEach } from 'vitest';
import type { JSX } from 'solid-js';
import { render } from 'solid-js/web';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import RunState, { runStateView, type RunStateName, type RunStateSize } from './RunState';
import { contrastRatio, relativeLuminance } from './Avatar';
import RunChips, { type RunChip } from '../runWithAgent/RunChips';
import type { StateTone } from '../runWithAgent/shared';

// The run-state tokens are plain CSS, so the AA check reads index.css and
// works the cascade out for each palette and mode, the way Avatar.test.tsx
// measures its own tokens. Every ink must clear 4.5:1 on its soft fill, and a
// translucent fill is composited over each surface it can sit on.

const MIN_AA_CONTRAST = 4.5;

const STATES: RunStateName[] = ['queued', 'running', 'needs', 'review', 'done', 'failed', 'cancelled'];
const SIZES: RunStateSize[] = ['dot', 'chip', 'banner'];
const SURFACES = ['bg-app', 'bg-base', 'bg-elevated', 'bg-subtle', 'bg-sidebar'];

const LABEL: Record<RunStateName, string> = {
  queued: 'Queued',
  running: 'Running',
  needs: 'Needs you',
  review: 'Ready for review',
  done: 'Finished',
  failed: 'Failed',
  cancelled: 'Cancelled',
};

const css = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), '../../index.css'), 'utf8');

interface Rule {
  /** 1 for `:root` or `.dark`, 2 with a `[data-palette]` attribute: the cascade's order of weight. */
  specificity: number;
  order: number;
  decls: Array<[name: string, value: string]>;
  palette?: string;
  dark: boolean;
}

/** Every rule that sets colours on `:root` or `.dark`, with and without a palette attribute. */
function themeRules(): Rule[] {
  const source = css.replace(/\/\*[\s\S]*?\*\//g, '');
  const rules: Rule[] = [];
  for (const match of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = match[1].trim();
    const parsed = /^(:root|\.dark)(?:\[data-palette="(\w+)"\])?$/.exec(selector);
    if (!parsed) continue;
    const decls = [...match[2].matchAll(/--([\w-]+)\s*:\s*([^;]+);/g)].map(
      (d): [string, string] => [d[1], d[2].trim()],
    );
    rules.push({
      specificity: parsed[2] ? 2 : 1,
      order: rules.length,
      decls,
      palette: parsed[2],
      dark: parsed[1] === '.dark',
    });
  }
  return rules;
}

const RULES = themeRules();

interface Theme {
  name: string;
  dark: boolean;
  palette?: string;
}

const THEMES: Theme[] = [
  { name: 'teal light', dark: false },
  { name: 'teal dark', dark: true },
  { name: 'clay light', dark: false, palette: 'clay' },
  { name: 'clay dark', dark: true, palette: 'clay' },
  { name: 'graphite light', dark: false, palette: 'graphite' },
  { name: 'graphite dark', dark: true, palette: 'graphite' },
  { name: 'harbor light', dark: false, palette: 'harbor' },
  { name: 'harbor dark', dark: true, palette: 'harbor' },
];

/** The custom properties a theme ends with: the matching rules, lowest specificity first, source order within. */
function themeVars(theme: Theme): Map<string, string> {
  const applies = RULES.filter((r) => {
    if (r.palette !== undefined && r.palette !== theme.palette) return false;
    if (r.dark && !theme.dark) return false;
    return true;
  });
  applies.sort((a, b) => a.specificity - b.specificity || a.order - b.order);
  const vars = new Map<string, string>();
  for (const rule of applies) for (const [name, value] of rule.decls) vars.set(name, value);
  return vars;
}

function resolveVar(name: string, vars: Map<string, string>, depth = 0): string {
  const raw = vars.get(name);
  if (raw === undefined) throw new Error(`--${name} is not defined for this theme`);
  if (depth > 8) throw new Error(`--${name} refers to itself`);
  return raw.replace(/var\(--([\w-]+)\)/g, (_, inner: string) => resolveVar(inner, vars, depth + 1));
}

type Rgba = [r: number, g: number, b: number, a: number];

function parseColor(value: string): Rgba {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const fn = /^rgba?\((.+)\)$/.exec(value);
  if (fn) {
    const [r, g, b, a = 1] = fn[1].split(/[\s,/]+/).filter(Boolean).map(Number);
    return [r, g, b, a];
  }
  throw new Error(`cannot read colour "${value}"`);
}

function luminanceOf([r, g, b]: Rgba): number {
  return relativeLuminance(r / 255, g / 255, b / 255);
}

/** A translucent colour painted over an opaque surface. */
function over([r, g, b, a]: Rgba, [br, bg, bb, ba]: Rgba): Rgba {
  if (ba !== 1) throw new Error('a surface must be opaque');
  return [r * a + br * (1 - a), g * a + bg * (1 - a), b * a + bb * (1 - a), 1];
}

describe('run-state tokens (index.css)', () => {
  it('the contrast helper measures what it claims (black on white is 21:1)', () => {
    expect(contrastRatio(luminanceOf([0, 0, 0, 1]), luminanceOf([255, 255, 255, 1]))).toBeCloseTo(21, 0);
  });

  it('holds every state ink to AA on its own soft fill, over every surface, in every palette and mode', () => {
    const failures: Array<{ theme: string; state: string; surface: string; contrast: number }> = [];
    let checked = 0;

    for (const theme of THEMES) {
      const vars = themeVars(theme);
      for (const state of STATES) {
        const ink = parseColor(resolveVar(`run-${state}-ink`, vars));
        if (ink[3] !== 1) throw new Error(`--run-${state}-ink must be opaque`);
        const soft = parseColor(resolveVar(`run-${state}-soft`, vars));
        for (const surface of SURFACES) {
          const ground = parseColor(resolveVar(`color-${surface}`, vars));
          const contrast = contrastRatio(luminanceOf(ink), luminanceOf(over(soft, ground)));
          checked++;
          if (contrast < MIN_AA_CONTRAST) failures.push({ theme: theme.name, state, surface, contrast });
        }
      }
    }

    expect(checked).toBe(THEMES.length * STATES.length * SURFACES.length);
    expect(failures, JSON.stringify(failures, null, 2)).toEqual([]);
  });

  it('turns the Running glyph only where the person has not asked for less motion', () => {
    expect(css).toMatch(
      /@media \(prefers-reduced-motion: no-preference\)\s*\{\s*:root:not\(\[data-motion="reduce"\]\) \.run-state-spin\s*\{\s*animation:\s*tk-spin/,
    );
  });
});

const disposers: Array<() => void> = [];

function mount(view: () => JSX.Element) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(view, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

describe('runStateView', () => {
  it('shows the state it was given, with no second line, when there is no docket word', () => {
    expect(runStateView({ state: 'running' })).toEqual({ state: 'running', word: undefined });
  });

  it('moves a run that waits for a person to Needs you, keeping the docket words', () => {
    expect(runStateView({ state: 'running', docket: 'Needs a person' })).toEqual({ state: 'needs', word: 'Needs a person' });
  });

  it('moves a finished run that waits for review to Ready for review', () => {
    expect(runStateView({ state: 'done', docket: 'Needs your review' })).toEqual({ state: 'review', word: 'Needs your review' });
  });

  it('keeps a cancellation requested on a run that is still going as Running', () => {
    expect(runStateView({ state: 'cancelled', docket: 'Cancellation requested' })).toEqual({
      state: 'running',
      word: 'Cancellation requested',
    });
  });

  it('keeps the state for "no changes recorded" and shows the words, capitalised', () => {
    expect(runStateView({ state: 'done', docket: 'no changes recorded' })).toEqual({
      state: 'done',
      word: 'No changes recorded',
    });
  });
});

describe('RunState', () => {
  for (const state of STATES) {
    it(`${state}: the chip shows its label and nothing else in text`, () => {
      const c = mount(() => <RunState state={state} />);
      const chip = c.querySelector('span') as HTMLElement;
      expect(chip.textContent).toBe(LABEL[state]);
      expect(chip.getAttribute('title')).toBe(LABEL[state]);
      expect(chip.style.color).toBe(`var(--run-${state}-ink)`);
    });

    it(`${state}: the dot is an icon named by its label`, () => {
      const c = mount(() => <RunState state={state} size="dot" />);
      const dot = c.querySelector('[role="img"]') as HTMLElement;
      expect(dot.getAttribute('aria-label')).toBe(LABEL[state]);
      expect(dot.getAttribute('title')).toBe(LABEL[state]);
      expect(dot.textContent).toBe('');
      expect(dot.querySelector('svg')?.getAttribute('aria-hidden')).toBe('true');
    });

    it(`${state}: the banner shows the label, with no second line without a docket word`, () => {
      const c = mount(() => <RunState state={state} size="banner" />);
      const paragraphs = [...c.querySelectorAll('p')].map((p) => p.textContent);
      expect(paragraphs).toEqual([LABEL[state]]);
      expect(c.querySelector('div')?.style.color).toBe(`var(--run-${state}-ink)`);
    });
  }

  it('gives each state its own glyph, the same at every size', () => {
    for (const size of SIZES) {
      const glyphs = STATES.map((state) => {
        const c = mount(() => <RunState state={state} size={size} />);
        return c.querySelector('svg')!.innerHTML;
      });
      expect(new Set(glyphs).size, size).toBe(STATES.length);
    }
  });

  it('marks only the Running glyph as the spinner', () => {
    for (const state of STATES) {
      const c = mount(() => <RunState state={state} />);
      const spins = c.querySelector('svg')!.classList.contains('run-state-spin');
      expect(spins, state).toBe(state === 'running');
    }
  });

  it('the banner carries the docket word as its second line', () => {
    const c = mount(() => <RunState state="running" docket="Needs a person" size="banner" />);
    const paragraphs = [...c.querySelectorAll('p')].map((p) => p.textContent);
    expect(paragraphs).toEqual(['Needs you', 'Needs a person']);
  });

  it('the dot names the docket word in its label', () => {
    const c = mount(() => <RunState state="running" docket="Needs a person" size="dot" />);
    const dot = c.querySelector('[role="img"]') as HTMLElement;
    expect(dot.getAttribute('aria-label')).toBe('Needs you — Needs a person');
  });

  it('the chip carries the docket state without the second line', () => {
    const c = mount(() => <RunState state="done" docket="Needs your review" />);
    expect(c.querySelector('span')!.textContent).toBe('Ready for review');
  });
});

describe('RunChips through RunState', () => {
  const dotLabel = (chip: RunChip) => {
    const c = mount(() => <RunChips chips={[chip]} active="" onPick={() => {}} label="Runs" testId="run-chips" />);
    return c.querySelector('[role="img"]')!.getAttribute('aria-label');
  };

  it('maps each chip to one of the seven states from its words, and tone where the words say nothing', () => {
    const cases: Array<[tone: StateTone, words: string, label: string]> = [
      ['neutral', 'Queued', 'Queued'],
      ['info', 'Starting', 'Running'],
      ['primary', 'Running', 'Running'],
      ['warning', 'Needs a person', 'Needs you'],
      ['warning', 'Needs operator', 'Needs you'],
      ['warning', 'Finished — needs your review', 'Ready for review'],
      ['warning', 'Finished — no changes recorded', 'Ready for review'],
      ['success', 'Finished', 'Finished'],
      ['success', 'Accepted', 'Finished'],
      ['danger', 'Failed', 'Failed'],
      ['danger', 'Lost', 'Failed'],
      ['neutral', 'Cancelled', 'Cancelled'],
      ['neutral', 'Rejected', 'Cancelled'],
    ];
    for (const [tone, words, label] of cases) {
      expect(dotLabel({ id: words, label: 'Run 1', tone, state: words }), words).toBe(label);
    }
  });

  it('keeps the chip words as its accessible name, and its pressed state', () => {
    const c = mount(() => (
      <RunChips
        chips={[{ id: 'a', label: 'Run 1', tone: 'neutral', state: 'Queued' }]}
        active="a"
        onPick={() => {}}
        label="Runs"
        testId="run-chips"
      />
    ));
    const button = c.querySelector('[data-testid="run-chip"]') as HTMLButtonElement;
    expect(button.textContent).toBe('Run 1, Queued');
    expect(button.getAttribute('aria-pressed')).toBe('true');
  });
});
