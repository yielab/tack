import { For, Show, type JSX } from 'solid-js';
import { Fold, HelpHint } from '../../shared/ui';
import { HELP } from '../../shared/help/texts';
import type { components } from '../../shared/api/schema.gen';

type WorkQuality = components['schemas']['WorkQuality'];
type Ratio = components['schemas']['Ratio'];

/** Who gave a verifier's word, as the figures name it. */
const VERIFIER_LABEL: Record<string, string> = {
  prill: 'Verifier pack',
  reviewer: 'Reviewer profile',
};

/** A rate as a percentage with its counts, or "Not measured" when there was nothing to count. */
function rate(r: Ratio): string {
  if (r.value === null || r.value === undefined) return 'Not measured';
  return `${(r.value * 100).toFixed(1)}% (${r.numerator} of ${r.denominator})`;
}

/** "1 task", "2 tasks". */
function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? '' : 's'}`;
}

function Table(props: { head: string[]; children: JSX.Element }) {
  return (
    <table class="w-full text-left text-xs text-content-subtle">
      <thead>
        <tr class="text-content-muted">
          <For each={props.head}>{(h) => <th class="font-normal">{h}</th>}</For>
        </tr>
      </thead>
      <tbody>{props.children}</tbody>
    </table>
  );
}

function Heading(props: { label: string; help: { text: string; href?: string } }) {
  return (
    <h3 class="m-0 mt-3 flex items-center gap-1 text-sm text-content">
      {props.label}
      <HelpHint label={props.label} help={props.help} />
    </h3>
  );
}

const CUT_LABEL: Record<string, string> = {
  profile: 'Profile',
  harness: 'Agent',
  model: 'Model',
  team: 'Team',
  template: 'Prompt fingerprint',
};

/** The quality of the work in the window, in small tables and folded away until asked for. */
export default function QualityFigures(props: { quality: WorkQuality }) {
  const q = () => props.quality;
  const passK = () => q().pass_pow_k;

  return (
    <Fold
      kind="metrics-quality"
      title="Quality of the work"
      summary={`Success ${rate(q().success_rate)}`}
      lazy
      testId="quality-figures"
    >
      <div class="flex flex-col gap-1">
        <Table head={['Figure', 'Value']}>
          <tr>
            <td class="text-content">
              Success <HelpHint label="Success" help={HELP.qualitySuccess} />
            </td>
            <td>{rate(q().success_rate)}</td>
          </tr>
          <tr>
            <td class="text-content">
              First-pass acceptance <HelpHint label="First-pass acceptance" help={HELP.qualityFirstPass} />
            </td>
            <td>{rate(q().first_pass_acceptance)}</td>
          </tr>
          <tr>
            <td class="text-content">
              Rework per task <HelpHint label="Rework per task" help={HELP.qualityRework} />
            </td>
            <td>
              {q().rework_per_item.value === null || q().rework_per_item.value === undefined
                ? 'Not measured'
                : `${q().rework_per_item.value!.toFixed(2)} (${count(q().rework_per_item.numerator, 'extra run')} over ${count(q().rework_per_item.denominator, 'task')})`}
            </td>
          </tr>
          <tr>
            <td class="text-content">
              Revert after merge <HelpHint label="Revert after merge" help={HELP.qualityRevert} />
            </td>
            <td>{rate(q().revert_after_merge)}</td>
          </tr>
          <tr>
            <td class="text-content">
              All {passK().k} runs of a recurring task <HelpHint label="Recurring tasks" help={HELP.qualityPassK} />
            </td>
            <td>
              {passK().value === null || passK().value === undefined
                ? 'Not measured'
                : `${(passK().value! * 100).toFixed(1)}% (${count(passK().tasks_measured, 'recurring task')})`}
            </td>
          </tr>
        </Table>

        <Heading label="Verifier agreement" help={HELP.qualityAgreement} />
        <Table head={['Who said it', 'False approvals', 'False rejections', 'Agreed', 'Left to you', 'No word']}>
          <For each={q().verifier_agreement}>
            {(row) => (
              <tr data-testid={`agreement-${row.verifier}`}>
                <td class="text-content">{VERIFIER_LABEL[row.verifier] ?? row.verifier}</td>
                <td>{rate(row.false_approval_rate)}</td>
                <td>{rate(row.false_rejection_rate)}</td>
                <td>{row.agreed}</td>
                <td>{row.handed_to_a_person}</td>
                <td>{row.without_a_word}</td>
              </tr>
            )}
          </For>
        </Table>

        <Heading label="Questions" help={HELP.qualityQuestions} />
        <Show when={q().questions.length > 0} fallback={<p class="text-xs text-content-muted">Nothing was asked in this window.</p>}>
          <Table head={['Agent', 'Kind', 'Asked', 'Per 100 steps', 'Allowed']}>
            <For each={q().questions}>
              {(row) => (
                <tr>
                  <td class="text-content">{row.harness}</td>
                  <td>{row.class}</td>
                  <td>{row.asked}</td>
                  <td>{row.per_100_actions === null || row.per_100_actions === undefined ? 'No steps' : row.per_100_actions.toFixed(1)}</td>
                  <td>{rate(row.granted)}</td>
                </tr>
              )}
            </For>
          </Table>
        </Show>

        <Heading label="Why runs stopped" help={HELP.qualityStops} />
        <Show when={q().stops.length > 0} fallback={<p class="text-xs text-content-muted">No run stopped in this window.</p>}>
          <Table head={['Reason', 'Runs']}>
            <For each={q().stops}>
              {(row) => (
                <tr>
                  <td class="text-content">
                    {row.reason}
                    <Show when={row.not_started}>
                      <span class="text-content-muted"> · not started</span>
                    </Show>
                  </td>
                  <td>{row.count}</td>
                </tr>
              )}
            </For>
          </Table>
        </Show>

        <Heading label="By who did the work" help={HELP.qualityCuts} />
        <For each={q().cuts.filter((c) => c.rows.length > 0)}>
          {(cut) => (
            <Table head={[CUT_LABEL[cut.by] ?? cut.by, 'Runs', 'Success', 'First pass', 'Reverted']}>
              <For each={cut.rows}>
                {(row) => (
                  <tr>
                    <td class="text-content">{row.name}</td>
                    <td>{row.attempts}</td>
                    <td>{rate(row.success_rate)}</td>
                    <td>{rate(row.first_pass_acceptance)}</td>
                    <td>{rate(row.revert_after_merge)}</td>
                  </tr>
                )}
              </For>
            </Table>
          )}
        </For>
      </div>
    </Fold>
  );
}
