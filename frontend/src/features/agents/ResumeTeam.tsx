import { type Component, Show, createSignal } from 'solid-js';
import { Button } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { resumeGate, resumedText, teamsApi, type TeamEntry, type TeamReport } from '../../shared/execution';

/**
 * Lets a paused team claim tasks again, through the computer that reports it. Drawn only for a paused
 * team; when the agent has no way to resume it the button stays, disabled, with the sentence that says
 * what to do instead. A refusal is shown as the agent worded it.
 */
const ResumeTeam: Component<{ entry: TeamEntry; onResumed: (report: TeamReport | null) => void }> = (props) => {
  const [busy, setBusy] = createSignal(false);
  const [refusal, setRefusal] = createSignal<string | null>(null);
  const gate = () => resumeGate(props.entry);

  const resume = async () => {
    const harness = props.entry.harness;
    if (!harness || busy()) return;
    setBusy(true);
    setRefusal(null);
    try {
      const resumed = await teamsApi.resume(harness, props.entry.name);
      const said = resumedText(props.entry.name, resumed.report);
      if (said.spent) toast.warning(said.text);
      else toast.success(said.text);
      props.onResumed(resumed.report ?? null);
    } catch (err) {
      setRefusal(err instanceof Error ? err.message : `${props.entry.name} could not be resumed.`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show when={props.entry.report?.paused}>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        loading={busy()}
        disabled={!gate().enabled}
        reason={gate().reason ?? undefined}
        aria-label={`Resume team ${props.entry.name}`}
        onClick={() => void resume()}
      >
        Resume team
      </Button>
      <Show when={refusal()}>
        {(text) => (
          <p role="alert" data-testid="team-resume-refused" class="w-full text-caption" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>
            {text()}
          </p>
        )}
      </Show>
    </Show>
  );
};

export default ResumeTeam;
