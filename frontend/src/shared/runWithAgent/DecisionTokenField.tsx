import { type Component, Show, createSignal } from 'solid-js';
import { Button, Field } from '../ui';
import { HELP } from '../help/texts';
import { toast } from '../ui/toast';
import { saveSessionDecisionToken, sessionDecisionToken } from '../state/decisionSession';

/**
 * Asks for the decision token, once for the browser session: when one was already given, in the
 * inbox or on another run, this is one line with a way to change it instead of another field.
 */
const DecisionTokenField: Component = () => {
  const [tokenInput, setTokenInput] = createSignal('');
  const [changing, setChanging] = createSignal(false);
  const given = () => sessionDecisionToken() !== null;

  const save = () => {
    saveSessionDecisionToken(tokenInput());
    setTokenInput('');
    setChanging(false);
    toast.success('Decision token saved for this browser session.');
  };

  return (
    <Show
      when={!given() || changing()}
      fallback={
        <p data-testid="decision-token-given" class="flex flex-wrap items-center gap-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Decision token given for this browser session.
          <Button size="sm" variant="ghost" onClick={() => setChanging(true)}>
            Change
          </Button>
        </p>
      }
    >
      <div class="flex flex-wrap items-end gap-2">
        <Field
          label="Your decision token"
          type="password"
          autocomplete="off"
          value={tokenInput()}
          onInput={(e) => setTokenInput(e.currentTarget.value)}
          hint="Ask your administrator for it. Kept only for this browser session."
          help={HELP.decisionToken}
          class="min-w-0 flex-1"
        />
        <Button size="sm" variant="secondary" onClick={save}>
          Save
        </Button>
      </div>
    </Show>
  );
};

export default DecisionTokenField;
