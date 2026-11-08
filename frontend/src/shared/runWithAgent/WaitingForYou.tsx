import { A, useLocation } from '@solidjs/router';
import { type Component, For, Show } from 'solid-js';
import type { Project } from '../types';
import { RunState, type RunStateName } from '../ui';
import { IconChevronDown, IconComment } from '../ui/icons';
import {
  WAITING_GROUPS,
  closeWaiting,
  openWaiting,
  revealWaiting,
  rows,
  waitingCount,
  waitingTally,
  waitingChangeSets,
  useWaiting,
  waitingOpen,
  type ChangeSetRow,
  type WaitingRow,
} from '../state/waiting';
import { sessionDecisionToken } from '../state/decisionSession';
import DecisionTokenField from './DecisionTokenField';
import { relativeTimeFromIso } from './shared';
import { tallySentence } from './cardRun';

/** What the count is made of: the work in its own words (the same tally as the board's Work strip), then the change sets; the sentence alone when nothing waits. */
const breakdown = (): string => {
  const changes = waitingChangeSets().length;
  const work = tallySentence(waitingTally());
  const parts = [
    work ? `Work: ${work}` : '',
    changes > 0 ? `${changes} ${changes === 1 ? 'change set' : 'change sets'} to apply` : '',
  ]
    .filter(Boolean)
    .join('; ');
  return parts ? `${waitingSentence(waitingCount())} across all projects: ${parts}` : waitingSentence(waitingCount());
};

/** The count in the words the board's Work strip uses, with its scope: the strip counts one project, this counts them all. */
const scopeLine = (): string => {
  const changes = waitingChangeSets().length;
  const parts = [tallySentence(waitingTally()), changes > 0 ? `${changes} ${changes === 1 ? 'change set' : 'change sets'} to apply` : ''].filter(Boolean);
  return parts.length > 0 ? `All projects: ${parts.join(', ')}` : '';
};

/** The sentence the count stands for, read aloud and shown on hover. */
function waitingSentence(count: number): string {
  if (count === 0) return 'Nothing is waiting for you';
  return count === 1 ? '1 thing is waiting for you' : `${count} things are waiting for you`;
}

const MEANING =
  'Questions an agent is waiting for you to answer, runs ready for your review, runs that failed since you last looked, and changes an agent proposed for you to apply.';

const countStyle = (count: number) => ({
  'min-width': '1.25rem',
  padding: '0 6px',
  'border-radius': 'var(--radius-pill)',
  'text-align': 'center' as const,
  'font-size': '0.75rem',
  'font-weight': 700,
  // The same amber as "Needs you" on the board: one thing that waits on a person has one colour.
  background: count > 0 ? 'var(--run-needs-soft)' : 'var(--color-bg-app)',
  color: count > 0 ? 'var(--run-needs-ink)' : 'var(--color-text-secondary)',
  'box-shadow': count > 0 ? 'inset 0 0 0 1px var(--run-needs-line)' : 'none',
});

const Row: Component<{ row: WaitingRow; project: string | undefined; action: string; state: RunStateName; onNavigate: () => void }> = (p) => {
  const location = useLocation();
  // The run opens on its Execution tab, over whatever page this is.
  const href = () => {
    const query = new URLSearchParams(location.search);
    query.set('item', p.row.itemId);
    query.set('tab', 'execution');
    query.set('run', p.row.requestId);
    return `${location.pathname}?${query.toString()}`;
  };
  const title = () => p.row.title ?? 'Title unavailable';
  return (
    <li data-testid="waiting-row" data-group={p.row.group} style={{ display: 'flex', gap: '8px', padding: '6px 4px', 'min-width': 0 }}>
      <span style={{ 'padding-top': '1px' }}>
        <RunState state={p.state} size="dot" />
      </span>
      <div style={{ flex: 1, 'min-width': 0 }}>
        <p
          title={title()}
          style={{ 'font-size': '0.8125rem', 'font-weight': 600, color: 'var(--color-text-primary)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}
        >
          {title()}
        </p>
        <p style={{ 'font-size': '0.75rem', color: 'var(--color-text-tertiary)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}>
          {p.project ? `${p.project} · ` : ''}
          {relativeTimeFromIso(p.row.at)}
        </p>
        <A
          href={href()}
          onClick={p.onNavigate}
          aria-label={`${p.action}: ${title()}`}
          style={{
            display: 'inline-block',
            'margin-top': '4px',
            padding: '2px 10px',
            'border-radius': 'var(--radius-pill)',
            'font-size': '0.75rem',
            'font-weight': 600,
            background: 'var(--color-accent-soft)',
            color: 'var(--color-accent-ink)',
          }}
        >
          {p.action}
        </A>
      </div>
    </li>
  );
};

/** A change set that waits: who proposed it and why, and the one action that opens it. */
const ChangeSetItem: Component<{ row: ChangeSetRow; project: string | undefined; onNavigate: () => void }> = (p) => {
  const what = () => `${p.row.proposer ?? 'An agent'} proposes ${p.row.operations} ${p.row.operations === 1 ? 'change' : 'changes'}`;
  return (
    <li data-testid="waiting-changeset" style={{ display: 'flex', gap: '8px', padding: '6px 4px', 'min-width': 0 }}>
      <span style={{ 'padding-top': '1px' }}>
        <RunState state="needs" size="dot" />
      </span>
      <div style={{ flex: 1, 'min-width': 0 }}>
        <p
          title={p.row.reason}
          style={{ 'font-size': '0.8125rem', 'font-weight': 600, color: 'var(--color-text-primary)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}
        >
          {what()}
        </p>
        <p style={{ 'font-size': '0.75rem', color: 'var(--color-text-tertiary)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}>
          {p.project ? `${p.project} · ` : ''}
          {relativeTimeFromIso(p.row.at)}
        </p>
        <A
          href={`/projects/${p.row.projectId}/change-sets?open=${p.row.id}`}
          onClick={p.onNavigate}
          aria-label={`Review the changes: ${what()}`}
          style={{
            display: 'inline-block',
            'margin-top': '4px',
            padding: '2px 10px',
            'border-radius': 'var(--radius-pill)',
            'font-size': '0.75rem',
            'font-weight': 600,
            background: 'var(--color-accent-soft)',
            color: 'var(--color-accent-ink)',
          }}
        >
          Review the changes
        </A>
      </div>
    </li>
  );
};

/**
 * The sidebar's inbox: everything that waits on a person, in four groups (the fourth is change sets to apply), with the primary action
 * of each run in its row. It stays in the sidebar when nothing waits, as a 0. Opening it is looking:
 * the next visit counts failures from now.
 */
const WaitingForYou: Component<{ projects: Project[] | undefined; onNavigate: () => void }> = (p) => {
  useWaiting();
  const nameOf = (id: string | null) => (id ? p.projects?.find((x) => x.id === id)?.name : undefined);
  const hasQuestions = () => rows().some((r) => r.group === 'ask');
  const toggle = () => (waitingOpen() ? closeWaiting() : openWaiting());

  return (
    <div data-testid="waiting-for-you" style={{ 'margin-bottom': '2px' }}>
      <button
        id="waiting-toggle"
        type="button"
        onClick={toggle}
        aria-expanded={waitingOpen()}
        aria-controls="waiting-panel"
        title={waitingCount() === 0 ? `${waitingSentence(0)}. ${MEANING}` : `${breakdown()}. ${MEANING}`}
        style={{
          width: '100%',
          display: 'flex',
          'align-items': 'center',
          gap: '10px',
          padding: '7px 12px',
          'border-radius': 'var(--radius-pill)',
          border: 'none',
          cursor: 'pointer',
          'font-family': 'inherit',
          'font-size': '0.875rem',
          'font-weight': 500,
          background: 'transparent',
          color: 'var(--color-text-primary)',
        }}
      >
        <IconComment size={16} />
        <span style={{ flex: 1, 'text-align': 'left', 'min-width': 0 }}>
          Waiting for you
          <Show when={scopeLine()}>
            <span
              data-testid="waiting-scope"
              style={{ display: 'block', 'font-size': '0.6875rem', 'font-weight': 400, 'line-height': 1.3, color: 'var(--color-text-secondary)' }}
            >
              {scopeLine()}
            </span>
          </Show>
        </span>
        <span data-testid="waiting-count" aria-hidden="true" style={countStyle(waitingCount())}>
          {waitingCount()}
        </span>
        <span class="sr-only">{waitingSentence(waitingCount())}</span>
        <span style={{ display: 'flex', transform: waitingOpen() ? 'rotate(180deg)' : 'none', color: 'var(--color-text-tertiary)' }}>
          <IconChevronDown size={13} />
        </span>
      </button>

      <Show when={waitingOpen()}>
        <div id="waiting-panel" role="region" aria-label="Waiting for you" style={{ padding: '2px 6px 8px', 'min-width': 0 }}>
          <Show when={waitingCount() === 0}>
            <p data-testid="waiting-empty" style={{ padding: '4px 6px', 'font-size': '0.75rem', color: 'var(--color-text-secondary)' }}>
              {waitingSentence(0)}.
            </p>
          </Show>
          <Show when={hasQuestions() && sessionDecisionToken() === null}>
            <div style={{ padding: '4px 6px 8px' }}>
              <DecisionTokenField />
            </div>
          </Show>
          <For each={WAITING_GROUPS}>
            {(group) => (
              <Show when={rows().some((r) => r.group === group.id)}>
                <section aria-label={group.label} data-testid={`waiting-group-${group.id}`}>
                  <h3
                    style={{
                      padding: '6px 6px 2px',
                      'font-size': '0.625rem',
                      'font-weight': 700,
                      'letter-spacing': '.12em',
                      'text-transform': 'uppercase',
                      color: 'var(--color-text-tertiary)',
                      'border-top': '1px solid var(--color-border-light)',
                    }}
                  >
                    {group.label}
                  </h3>
                  <ul>
                    <For each={rows().filter((r) => r.group === group.id)}>
                      {(row) => (
                        <Row
                          row={row}
                          project={nameOf(row.projectId)}
                          action={group.action}
                          state={group.state}
                          onNavigate={p.onNavigate}
                        />
                      )}
                    </For>
                  </ul>
                </section>
              </Show>
            )}
          </For>
          <Show when={waitingChangeSets().length > 0}>
            <section aria-label="Changes to apply" data-testid="waiting-group-changes">
              <h3
                style={{
                  padding: '6px 6px 2px',
                  'font-size': '0.625rem',
                  'font-weight': 700,
                  'letter-spacing': '.12em',
                  'text-transform': 'uppercase',
                  color: 'var(--color-text-tertiary)',
                  'border-top': '1px solid var(--color-border-light)',
                }}
              >
                Changes to apply
              </h3>
              <ul>
                <For each={waitingChangeSets()}>
                  {(row) => <ChangeSetItem row={row} project={nameOf(row.projectId)} onNavigate={p.onNavigate} />}
                </For>
              </ul>
            </section>
          </Show>
        </div>
      </Show>
    </div>
  );
};

/**
 * The badge's accessible name. Its visible text is the count, so the name begins with it
 * (WCAG 2.5.3 Label in Name) — the sentence alone says "Nothing" where the screen shows "0".
 */
const badgeLabel = (count: number): string => (count === 0 ? `0: ${waitingSentence(0)}` : waitingSentence(count));

/** The top bar's indicator: the same count, and a press brings the list into view. */
export const WaitingBadge: Component = () => {
  useWaiting();
  const reveal = () => {
    revealWaiting();
    setTimeout(() => document.getElementById('waiting-toggle')?.focus(), 0);
  };
  return (
    <button
      type="button"
      data-testid="waiting-badge"
      onClick={reveal}
      aria-label={badgeLabel(waitingCount())}
      title={breakdown()}
      class="inline-flex items-center gap-1.5"
      style={{
        padding: '5px 10px',
        'border-radius': 'var(--radius-pill)',
        cursor: 'pointer',
        border: '1px solid var(--color-border-light)',
        background: 'transparent',
        color: 'var(--color-text-secondary)',
      }}
    >
      <IconComment size={15} />
      <span aria-hidden="true" style={countStyle(waitingCount())}>
        {waitingCount()}
      </span>
    </button>
  );
};

export default WaitingForYou;
