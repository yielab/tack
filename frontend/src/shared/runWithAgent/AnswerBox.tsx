import { type Component, For, Show, createSignal } from 'solid-js';
import { Button, Field } from '../ui';
import type { ButtonVariant } from '../ui';
import type { DecisionAnswer, DecisionRecord } from '../execution';
import RadioRow from './RadioRow';
import { answerChoices, answerTextOf } from './answerParts';

export interface AnswerBoxProps {
  decision: DecisionRecord;
  /** Sends the answer; rejects when it was not taken, and the box keeps what was typed. */
  onSend: (answer: DecisionAnswer) => Promise<void>;
}

/**
 * The answer to one pending question, in the order a person reads it. A question lists its choices
 * (the recommended one first, marked once, with its reason), the field opening under the choice it
 * belongs to, and one button that sends. An approval is its choices as buttons: the recommended one
 * solid, the others outlined, the refusal in red, the field opening when the refusal takes a reason.
 * The way out is a plain action beside them. A question that takes no text has no field; a question
 * with no choices is its field alone.
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

  // An approval answers with a press: no choice to tick first, then a button to send.
  const asButtons = () => props.decision.kind === 'approval' && !freeForm() && set().choices.length > 0 && !soleAnswer();
  const lead = () => set().recommendedId || set().choices.find((o) => !owns(o.option_id))?.option_id || '';
  const variantOf = (id: string): ButtonVariant => (owns(id) ? 'destructive' : id === lead() ? 'primary' : 'secondary');
  /** A choice that takes text first opens its field; pressing it again, with the text, sends it. */
  const press = (id: string) => {
    if (owns(id) && selected() !== id) {
      pick(id);
      return;
    }
    const value = typed().trim();
    if (owns(id) && text()?.required && value === '') return;
    void send({ option_id: id, text: owns(id) && value ? value : null });
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
      <Show when={asButtons()}>
        <Show when={set().recommendedId && props.decision.recommendation?.rationale}>
          {(rationale) => (
            <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
              <span class="font-semibold" style={{ color: 'var(--color-accent-ink)' }}>Recommended</span>{' '}
              {rationale()}
              <Show when={(props.decision.recommendation?.evidence_refs ?? []).length > 0}> ({(props.decision.recommendation?.evidence_refs ?? []).join(', ')})</Show>
            </p>
          )}
        </Show>
        <ul class="space-y-1">
          <For each={set().choices}>
            {(opt) => {
              const quiet = () =>
                [
                  opt.description ?? '',
                  opt.risks && opt.risks.length > 0 ? `Risk: ${opt.risks.join('; ')}` : '',
                  opt.estimated_tokens != null ? `About ${opt.estimated_tokens.toLocaleString('en-US')} tokens` : '',
                ].filter(Boolean).join(' · ');
              return (
                <Show when={quiet()}>
                  <li class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
                    <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{opt.label}</span>: {quiet()}
                  </li>
                </Show>
              );
            }}
          </For>
        </ul>
        <Show when={set().choices.some((o) => selected() === o.option_id && owns(o.option_id))}>{field()}</Show>
        <div class="flex flex-wrap items-center gap-2" role="group" aria-label="Answers">
          <For each={set().choices}>
            {(opt) => (
              <Button
                type="button"
                variant={variantOf(opt.option_id)}
                disabled={busy() || (owns(opt.option_id) && selected() === opt.option_id && required() && typed().trim() === '')}
                reason={busy() ? undefined : owns(opt.option_id) && selected() === opt.option_id && required() && typed().trim() === '' ? 'Write your answer to send it' : undefined}
                data-option={opt.option_id}
                onClick={() => press(opt.option_id)}
              >
                {opt.label}
              </Button>
            )}
          </For>
          <Show when={set().decline}>
            {(decline) => (
              <Button type="button" variant="ghost" disabled={busy()} onClick={() => void send({ option_id: decline().option_id, text: null })}>
                {decline().label}
              </Button>
            )}
          </Show>
        </div>
      </Show>
      <Show when={!asButtons()}>
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
                          <span class="block text-small" style={{ color: 'var(--color-text-secondary)' }}>
                            {opt.description}
                          </span>
                        </Show>
                        <Show when={recommended() && rationale()}>
                          <span class="block text-small" style={{ color: 'var(--color-text-secondary)' }}>
                            {rationale()}
                            <Show when={evidence()}> ({evidence()})</Show>
                          </span>
                        </Show>
                        <Show when={quiet()}>
                          <span class="block text-small" style={{ color: 'var(--color-text-secondary)' }}>
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
      </Show>
    </form>
  );
};

export default AnswerBox;
