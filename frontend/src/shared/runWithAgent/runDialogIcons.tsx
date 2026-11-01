import { type Component, type JSX, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { AgentCapabilityId } from '../execution/agents';

// The glyphs the run dialog draws in place of sentences. Each is decorative on its own: the caller
// wraps it with the words a screen reader and a tooltip need.

interface GlyphProps {
  size?: number;
}

function glyph(body: () => JSX.Element): Component<GlyphProps> {
  return (props) => (
    <svg
      width={props.size ?? 14}
      height={props.size ?? 14}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.7"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
      class="flex-none"
    >
      {body()}
    </svg>
  );
}

export const IconBranch = glyph(() => (
  <>
    <circle cx="4.5" cy="3.5" r="1.5" />
    <circle cx="4.5" cy="12.5" r="1.5" />
    <circle cx="11.5" cy="6" r="1.5" />
    <path d="M4.5 5v6M11.5 7.5c0 2.5-3 2-7 3.5" />
  </>
));

export const IconFolder = glyph(() => <path d="M2 4.5h4l1.5 1.5H14v6.5H2z" />);

export const IconPackage = glyph(() => (
  <>
    <path d="M8 2l5.5 3v6L8 14l-5.5-3V5z" />
    <path d="M2.5 5L8 8l5.5-3M8 8v6" />
  </>
));

export const IconChecks = glyph(() => (
  <>
    <path d="M2.5 4.5l1.3 1.3 2.2-2.4M2.5 10.5l1.3 1.3 2.2-2.4" />
    <path d="M8.5 5h5M8.5 11h5" />
  </>
));

export const IconShield = glyph(() => <path d="M8 2l5 2v4c0 3-2.2 5-5 6-2.8-1-5-3-5-6V4z" />);

export const IconGlobe = glyph(() => (
  <>
    <circle cx="8" cy="8" r="6" />
    <path d="M2 8h12M8 2c2.2 2.2 2.2 9.8 0 12M8 2c-2.2 2.2-2.2 9.8 0 12" />
  </>
));

export const IconWho = glyph(() => (
  <>
    <circle cx="8" cy="5.5" r="2.5" />
    <path d="M3 13.5c.6-2.6 2.5-4 5-4s4.4 1.4 5 4" />
  </>
));

const CAPABILITY_GLYPH: Record<AgentCapabilityId, Component<GlyphProps>> = {
  cancel: glyph(() => <rect x="4" y="4" width="8" height="8" rx="1.5" />),
  resume: glyph(() => <path d="M13 8a5 5 0 1 1-1.8-3.8M13 2.5v3h-3" />),
  decisions: glyph(() => <path d="M3 3.5h10v7H8l-3 2.5v-2.5H3z" />),
  proposals: glyph(() => <path d="M3 4.5h6M3 8.5h4M11.5 7v6M8.5 10h6" />),
  usage: glyph(() => <path d="M4 13V8M8 13V3.5M12 13V6" />),
};

/** The icon for one thing an agent can do; `struck` draws the diagonal of one it cannot or did not report. */
export const CapabilityGlyph: Component<{ id: AgentCapabilityId; struck?: boolean }> = (props) => {
  return (
    <span class="relative inline-flex h-4 w-4 flex-none items-center justify-center">
      <Dynamic component={CAPABILITY_GLYPH[props.id]} />
      <Show when={props.struck}>
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.7"
          stroke-linecap="round"
          aria-hidden="true"
          class="absolute inset-0"
        >
          <path d="M2.5 13.5L13.5 2.5" />
        </svg>
      </Show>
    </span>
  );
};
