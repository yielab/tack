import { type Component, For, Show, createSignal } from 'solid-js';
import { Button, Field } from '../ui';
import type { DecisionAnswer, DecisionRecord } from '../execution';
import RadioRow from './RadioRow';
import { answerChoices, answerTextOf } from './answerParts';

export interface AnswerBoxProps {
  decision: DecisionRecord;
  /** Sends the answer; rejects when it was not taken, and the box keeps what was typed. */
  onSend: (answer: DecisionAnswer) => Promise<void>;
}

/**
 * The answer to one pending question, in the order a person reads it: the choices (the recommended
 * one first, marked once, with its reason), the field opening under the choice it belongs to and
 * named for what is asked, one button that sends, and the way out as a plain action beside it.
 * Approvals and questions are the same box, so they read alike. A question that takes no text has
 * no field; a question with no choices is its field alone.
 */
const AnswerBox: Component<AnswerBoxProps> = (props) => {
  const set = () => answerChoices(props.decision);
  const text = () => answerTextOf(props.decision.metadata);
  const freeForm = () => props.decision.options.length === 0;
  const initial = () => {
    const { choices, recommendedId } = set();
    return recommendedId || (choices.length === 1 ? choices[0].option_id : '');
  };
  const [selected, setSelected] = createSignal(initial());
  const [typed, setTyped] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const owns = (id: string) => text()?.options.includes(id) ?? false;
  // The one choice there is takes the text: the field is the answer, with no choice drawn above it.
  const soleAnswer = () => set().choices.length === 1 && owns(set().choices[0].option_id);
  const showsField = () => freeForm() || owns(selected());
  const required = () => freeForm() || (text()?.required === true && owns(selected()));
  const fieldLabel = () => (freeForm() ? 'Your answer' : (text()?.label ?? 'Your answer'));
  const fieldId = () => `answer-${props.decision.decision_id}`;
  const canSend = () => (freeForm() ? true : selected() !== '') && !(required() && typed().trim() === '');

  const send = async (answer: DecisionAnswer) => {
    if (busy()) return;
    setBusy(true);
    try {
      await props.onSend(answer);
    } catch {
      /* already said by the caller; what was typed stays */
    } finally {
      setBusy(false);
    }
  };

  const submit = (e: Event) => {
    e.preventDefault();
    if (!canSend()) return;
    const value = typed().trim();
    void send(freeForm() ? { option_id: value, text: null } : { option_id: selected(), text: showsField() && value ? value : null });
  };

  const pick = (id: string) => {
    setSelected(id);
    // The field the choice opens takes the cursor, so the answer is typed at once.
    if (owns(id)) queueMicrotask(() => document.getElementById(fieldId())?.focus());
  };

  const field = () => (
    <Field
      id={fieldId()}
      label={fieldLabel()}
      value={typed()}
      onInput={(e) => setTyped(e.currentTarget.value)}
      required={required()}
    />
  );

  return (
    <form class="space-y-3" onSubmit={submit}>
      <Show
        when={!freeForm()}
        fallback={field()}
      >
        <Show when={soleAnswer()} fallback={
          <Show when={set().choices.length > 0}>
            <fieldset class="flex flex-col gap-2">
              <legend class="sr-only">Answers</legend>
              <For each={set().choices}>
                {(opt) => {
                  const recommended = () => opt.option_id === set().recommendedId;
                  const rationale = () => props.decision.recommendation?.rationale;
                  const evidence = () => (props.decision.recommendation?.evidence_refs ?? []).join(', ');
                  const quiet = () =>
                    [
                      opt.risks && opt.risks.length > 0 ? `Risk: ${opt.risks.join('; ')}` : '',
                      opt.estimated_tokens != null ? `About ${opt.estimated_tokens.toLocaleString('en-US')} tokens` : '',
                    ]
                      .filter(Boolean)
                      .join(' · ');
                  return (
                    <>
                      <RadioRow
                        name={`decision-${props.decision.decision_id}`}
                        value={opt.option_id}
                        checked={selected() === opt.option_id}
                        onChange={() => pick(opt.option_id)}
                      >
                        <span class="font-semibold">{opt.label}</span>
                        <Show when={recommended()}>
                          <span class="ml-2 text-micro font-bold uppercase tracking-[0.1em]" style={{ color: 'var(--color-accent-ink)' }}>
                            Recommended
                          </span>
                        </Show>
                        <Show when={opt.description}>
                          <span class="block text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                            {opt.description}
                          </span>
                        </Show>
                        <Show when={recommended() && rationale()}>
                          <span class="block text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                            {rationale()}
                            <Show when={evidence()}> ({evidence()})</Show>
                          </span>
                        </Show>
                        <Show when={quiet()}>
                          <span class="block text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                            {quiet()}
                          </span>
                        </Show>
                      </RadioRow>
                      <Show when={selected() === opt.option_id && owns(opt.option_id)}>
                        <div class="pl-4">{field()}</div>
                      </Show>
                    </>
                  );
                }}
              </For>
            </fieldset>
          </Show>
        }>
          {field()}
        </Show>
      </Show>
      <div class="flex flex-wrap items-center gap-2">
        <Show when={freeForm() || set().choices.length > 0}>
          <Button
            type="submit"
            size="sm"
            disabled={busy() || !canSend()}
            loading={busy()}
            reason={busy() || canSend() ? undefined : selected() === '' && !freeForm() ? 'Pick one to send' : 'Write your answer to send it'}
          >
            Send answer
          </Button>
        </Show>
        <Show when={set().decline}>
          {(decline) => (
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={busy()}
              onClick={() => void send({ option_id: decline().option_id, text: null })}
            >
              {decline().label}
            </Button>
          )}
        </Show>
      </div>
    </form>
  );
};

export default AnswerBox;
