import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render } from 'solid-js/web';
import RateCardForm from './RateCardForm';

const flush = () => new Promise((r) => setTimeout(r, 0));

const CARD = {
  as_of: '2026-10-01',
  ci_minute: 0.008,
  human_hour: 60,
  models: {
    'claude-sonnet-4-6': { input_per_mtok: 3, output_per_mtok: 15 },
    'gpt-5': { input_per_mtok: 1.25, output_per_mtok: 10 },
  },
};

let fetchMock: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  fetchMock = vi.spyOn(globalThis, 'fetch');
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount(onSaved: () => void = () => {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <RateCardForm onSaved={onSaved} />, container);
  return container;
}

function inputs(container: HTMLElement): HTMLInputElement[] {
  return Array.from(container.querySelectorAll('input'));
}

describe('RateCardForm', () => {
  it('round-trips: loads the card, edits one price, and saves the same shape', async () => {
    const bodies: unknown[] = [];
    fetchMock.mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith('/api/settings/rate-card') && init?.method === 'PUT') {
        bodies.push(JSON.parse(String(init.body)));
        return Promise.resolve(new Response(init.body as string, { status: 200 }));
      }
      if (url.endsWith('/api/settings/rate-card')) {
        return Promise.resolve(new Response(JSON.stringify(CARD), { status: 200 }));
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const container = mount();
    await flush();
    await flush();

    const values = inputs(container).map((i) => i.value);
    expect(values).toContain('claude-sonnet-4-6');
    expect(values).toContain('gpt-5');
    expect(values).toContain('2026-10-01');

    // Change the sonnet input price from 3 to 4.
    const sonnetRow = inputs(container).findIndex((i) => i.value === 'claude-sonnet-4-6');
    const inputPrice = inputs(container)[sonnetRow + 1];
    inputPrice.value = '4';
    inputPrice.dispatchEvent(new Event('input', { bubbles: true }));

    container.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await flush();
    await flush();

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toEqual({
      ...CARD,
      models: {
        'claude-sonnet-4-6': { input_per_mtok: 4, output_per_mtok: 15 },
        'gpt-5': { input_per_mtok: 1.25, output_per_mtok: 10 },
      },
    });
  });

  it('with no card yet, shows an empty form with one blank model row', async () => {
    fetchMock.mockImplementation(() => Promise.resolve(new Response('null', { status: 200 })));

    const container = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('No rate card yet');
    expect(container.querySelectorAll('tbody tr')).toHaveLength(1);
  });
});
