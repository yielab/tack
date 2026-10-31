import { type Component, type JSX, For, Show } from 'solid-js';
import { Button } from '../../shared/ui';
import { currentTheme, setTheme, type Theme } from '../../shared/state/theme';
import { currentPalette, setPalette, PALETTE_SWATCHES } from '../../shared/state/palette';
import {
  currentDisplay,
  setDisplay,
  resetDisplay,
  zoomSupported,
  TEXT_SIZES,
  ZOOM_LEVELS,
} from '../../shared/state/display';

const THEMES: { value: Theme; label: string }[] = [
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'system', label: 'System' },
];

const percent = (n: number) => `${Math.round(n * 100)}%`;

const systemReducesMotion = () =>
  typeof window !== 'undefined' &&
  typeof window.matchMedia === 'function' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/**
 * One choice out of several, as native radios in a fieldset: arrow keys move
 * between options and a screen reader announces the group and the position.
 * The radio itself is visually hidden; the label is the pill the user sees.
 */
function ChoiceGroup<T extends string | number>(p: {
  legend: string;
  name: string;
  options: { value: T; label: JSX.Element; aria?: string }[];
  value: T;
  onChange: (v: T) => void;
  disabled?: boolean;
  hint?: JSX.Element;
}) {
  const hintId = `${p.name}-hint`;
  return (
    <fieldset class="flex flex-col gap-2" disabled={p.disabled} aria-describedby={p.hint ? hintId : undefined}>
      <legend class="mb-2 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
        {p.legend}
      </legend>
      <div
        class="inline-flex flex-wrap gap-[3px] self-start rounded-xl p-[3px] text-small"
        style={{ 'background-color': 'var(--color-bg-app)', opacity: p.disabled ? 0.6 : 1 }}
      >
        <For each={p.options}>
          {(o) => {
            const checked = () => p.value === o.value;
            return (
              <label
                class="relative inline-flex items-center gap-2 rounded-full px-4 py-1.5 transition-colors has-[:focus-visible]:ring-2"
                style={{
                  cursor: p.disabled ? 'not-allowed' : 'pointer',
                  'background-color': checked() ? 'var(--color-primary-600)' : 'transparent',
                  color: checked() ? 'var(--color-on-accent)' : 'var(--color-text-secondary)',
                  'font-weight': checked() ? 700 : 500,
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              >
                <input
                  type="radio"
                  class="sr-only"
                  name={p.name}
                  value={String(o.value)}
                  checked={checked()}
                  aria-label={o.aria}
                  onChange={() => p.onChange(o.value)}
                />
                {o.label}
              </label>
            );
          }}
        </For>
      </div>
      <Show when={p.hint}>
        <p id={hintId} class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
          {p.hint}
        </p>
      </Show>
    </fieldset>
  );
}

/** An on/off preference as a native checkbox with its explanation underneath. */
const Toggle: Component<{
  id: string;
  label: string;
  hint: JSX.Element;
  checked: boolean;
  onChange: (v: boolean) => void;
}> = (p) => (
  <div class="flex items-start gap-3">
    <input
      id={p.id}
      type="checkbox"
      class="mt-0.5 h-5 w-5 flex-none cursor-pointer rounded"
      style={{ 'accent-color': 'var(--color-primary-600)' }}
      checked={p.checked}
      aria-describedby={`${p.id}-hint`}
      onChange={(e) => p.onChange(e.currentTarget.checked)}
    />
    <div class="flex flex-col gap-0.5">
      <label for={p.id} class="cursor-pointer text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
        {p.label}
      </label>
      <p id={`${p.id}-hint`} class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
        {p.hint}
      </p>
    </div>
  </div>
);

/**
 * Settings → Display & accessibility: how the interface looks and reads for
 * this person — theme, palette, text size, zoom, typeface and motion. Saved
 * to this browser, like the theme always was.
 */
const DisplaySettings: Component<{ cardClass: string; titleClass: string }> = (p) => (
  <section id="display" class={p.cardClass} aria-labelledby="display-title">
    <div class="flex flex-wrap items-center gap-3">
      <h2 id="display-title" class={p.titleClass} style={{ color: 'var(--color-text-primary)' }}>
        Display &amp; accessibility
      </h2>
      <Button size="sm" variant="ghost" class="ml-auto" onClick={() => { resetDisplay(); setTheme('system'); setPalette('harbor'); }}>
        Reset to defaults
      </Button>
    </div>
    <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
      Adapt the interface to how you read it. Saved to this browser.
    </p>

    <div class="flex flex-col gap-5 pt-1">
      <ChoiceGroup
        legend="Theme"
        name="theme"
        options={THEMES}
        value={currentTheme()}
        onChange={setTheme}
        hint="System follows your operating system's light or dark setting."
      />

      <ChoiceGroup
        legend="Palette"
        name="palette"
        options={PALETTE_SWATCHES.map(([value, label, color]) => ({
          value,
          label: (
            <>
              <span
                aria-hidden="true"
                class="h-3.5 w-3.5 flex-none rounded-full"
                style={{ background: color, 'box-shadow': '0 0 0 2px var(--color-bg-base)' }}
              />
              {label}
            </>
          ),
        }))}
        value={currentPalette()}
        onChange={setPalette}
      />

      <ChoiceGroup
        legend="Text size"
        name="text-size"
        options={TEXT_SIZES.map((s) => ({ value: s, label: percent(s) }))}
        value={currentDisplay().textSize}
        onChange={(textSize) => setDisplay({ textSize })}
        hint="Scales every piece of text, on top of the size your browser or system already sets."
      />

      <ChoiceGroup
        legend="Zoom"
        name="zoom"
        options={ZOOM_LEVELS.map((z) => ({ value: z, label: percent(z) }))}
        value={zoomSupported() ? currentDisplay().zoom : 1}
        onChange={(zoom) => setDisplay({ zoom })}
        disabled={!zoomSupported()}
        hint={
          zoomSupported()
            ? 'Scales the whole interface. Ctrl + and Ctrl − (⌘ on a Mac) also zoom, and Ctrl 0 resets.'
            : 'In a browser tab, use the browser’s own zoom: Ctrl + and Ctrl − (⌘ on a Mac), Ctrl 0 to reset. This setting drives the desktop app.'
        }
      />

      <Toggle
        id="legible-font"
        label="High-legibility font"
        hint="Switches every text to Atkinson Hyperlegible, drawn so similar letters and digits (I l 1, O 0) stay distinct."
        checked={currentDisplay().legibleFont}
        onChange={(legibleFont) => setDisplay({ legibleFont })}
      />

      <Toggle
        id="reduce-motion"
        label="Reduce motion"
        hint={
          systemReducesMotion()
            ? 'Your system already asks for reduced motion, so animations are off either way.'
            : 'Turns off slide, fade and pulse animations.'
        }
        checked={currentDisplay().motion === 'reduce' || systemReducesMotion()}
        onChange={(on) => setDisplay({ motion: on ? 'reduce' : 'system' })}
      />
    </div>
  </section>
);

export default DisplaySettings;
