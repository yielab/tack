import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import SignInStep from './SignInStep';
import type { RunnerSummary } from '../../../shared/execution/api';

afterEach(() => {
  document.body.innerHTML = '';
});

const runner = (harnesses: unknown[]): RunnerSummary =>
  ({
    runner_id: 'r1', name: 'This computer', state: 'active', labels: null, labels_raw: '{}', total_capacity: 1, available_capacity: 1,
    capability_snapshot: { harnesses }, capability_snapshot_raw: '', protocol_version: 1, runner_version: '1', last_heartbeat_at: null,
    online: true, revoked_at: null, fleet_ids: [], created_at: '', updated_at: '',
  }) as RunnerSummary;

const harness = (kind: string, sign_in: unknown) => ({ harness_kind: kind, installed_version: '1.0', probe_error: null, probed_at: '', sign_in });

function mount(harnesses: unknown[]) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <SignInStep thisMachineRunner={runner(harnesses)} />, container);
  return container;
}

const row = (c: HTMLElement, kind: string) => c.querySelector<HTMLElement>(`[data-testid="sign-in-${kind}"]`)!;
const trouble = (c: HTMLElement, kind: string) => c.querySelector<HTMLElement>(`[data-testid="sign-in-trouble-${kind}"]`);

describe('SignInStep', () => {
  it('turns a failed probe into a warning row: the command to run in a code block and the raw message in a fold', () => {
    const detail = '`opencode debug config` exited with status 1';
    const c = mount([harness('opencode', { state: 'unknown', detail, command: 'opencode auth login' })]);
    const opencode = row(c, 'opencode');
    expect(opencode.getAttribute('data-sign-in')).toBe('unknown');
    expect(opencode.textContent).toContain("Can't tell");
    expect(opencode.textContent).toContain("OpenCode can't say whether it is signed in.");

    const blocks = Array.from(trouble(c, 'opencode')!.querySelectorAll('pre > code')).map((code) => code.textContent);
    expect(blocks).toEqual(['opencode debug config', 'opencode auth login']);

    const fold = trouble(c, 'opencode')!.querySelector('details')!;
    expect(fold.open).toBe(false);
    expect(fold.querySelector('summary')!.textContent).toBe('What OpenCode said');
    expect(fold.querySelector('p')!.textContent).toBe(detail);
    // The raw message is only in the fold, not in the row's own sentence.
    expect(opencode.textContent!.split(detail)).toHaveLength(2);
  });

  it('keeps a message that names no command in the fold, with no code block', () => {
    const c = mount([harness('opencode', { state: 'unknown', detail: 'opencode does not say whether it is signed in', command: null })]);
    expect(trouble(c, 'opencode')!.querySelector('pre')).toBeNull();
    expect(trouble(c, 'opencode')!.querySelector('details p')!.textContent).toBe('opencode does not say whether it is signed in');
  });

  it('shows nothing extra for an agent that is signed in or has no sign-in report', () => {
    const c = mount([harness('claude-code', { state: 'signed_in', detail: 'Logged in', command: null }), harness('codex', null)]);
    expect(trouble(c, 'claude-code')).toBeNull();
    expect(row(c, 'claude-code').textContent).toContain('Signed in');
    expect(trouble(c, 'codex')).toBeNull();
    expect(row(c, 'codex').textContent).toContain("Codex can't say whether it is signed in.");
  });
});
