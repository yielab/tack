import type { Component } from 'solid-js';
import Button from './Button';
import { toast } from './toast';

/** A fix a person runs elsewhere (a command, or a sentence that ends in one), with a button that copies it. */
const CopyFix: Component<{ fix: string; what: string }> = (props) => {
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(props.fix);
      toast.success('Copied.');
    } catch {
      toast.error(`Couldn't copy it: ${props.fix}`);
    }
  };
  return (
    <span class="inline-flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
      <code data-testid="copy-fix-text" class="min-w-0 font-mono text-caption" style={{ 'overflow-wrap': 'anywhere', color: 'var(--color-text-primary)' }}>
        {props.fix}
      </code>
      <Button type="button" variant="secondary" size="sm" aria-label={`Copy the fix for ${props.what}`} onClick={() => void copy()}>
        Copy
      </Button>
    </span>
  );
};

export default CopyFix;
