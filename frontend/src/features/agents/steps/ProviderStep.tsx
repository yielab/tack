import type { Component } from 'solid-js';
import ProviderKeyPanel from '../ProviderKeyPanel';

/**
 * Step 3 — agents sign in on their own (ADR 0074 decision 1); a gateway key
 * is the optional alternative, so `ProviderKeyPanel` sits beneath unchanged.
 */
const ProviderStep: Component = () => {
  return (
    <section class="flex flex-col gap-2.5">
      <h2 class="pt-2 text-[20px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
        Provider
      </h2>
      <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
        Your agents sign in on their own: run <code>claude</code> or <code>codex</code> once in a terminal and Tack
        uses that login. A provider key is only needed if you want runs to go through a gateway (Vercel AI Gateway)
        instead of the agent's own account.
      </p>
      <div class="flex flex-col gap-2.5 rounded-[28px] bg-panel px-5 py-[18px]">
        <ProviderKeyPanel />
        <p class="text-[11.5px]" style={{ color: 'var(--color-text-tertiary)' }}>
          This is this one provider's own catalog — the count above is not the full
          picture of every model this machine can reach.
        </p>
      </div>
    </section>
  );
};

export default ProviderStep;
