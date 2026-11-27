import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import KeyLabel from './KeyLabel';

afterEach(() => { document.body.innerHTML = ''; });

describe('KeyLabel', () => {
  it('shows a row\'s key, and nothing for a row the board sent without one', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    render(() => <><KeyLabel row={{ name: 'Dev Team', key: 'dev-team' }} /><KeyLabel row={{ name: 'Old' }} /><KeyLabel row={{ key: '' }} /></>, container);
    const keys = Array.from(container.querySelectorAll('[data-testid="row-key"]')).map((k) => k.textContent);
    expect(keys).toEqual(['dev-team']);
  });
});
