import { describe, it, expect } from 'vitest';
import { resolveLabel, VOCAB_KEYS } from './vocab';

describe('resolveLabel', () => {
  it('returns the vocab override when the key is present', () => {
    expect(resolveLabel({ sprint: 'Iteration' }, 'sprint')).toBe('Iteration');
  });

  it('falls back to the built-in default when the key is not in vocab', () => {
    expect(resolveLabel({}, 'backlog')).toBe('Backlog');
    expect(resolveLabel(undefined, 'sprint')).toBe('Sprint');
  });

  it('returns the key itself for an unknown vocab key', () => {
    expect(resolveLabel({}, 'unknown_key')).toBe('unknown_key');
  });

  it('applies construction-style overrides across multiple keys', () => {
    const vocab = { sprint: 'Phase', backlog: 'Pending Work', milestone: 'Inspection Point' };
    expect(resolveLabel(vocab, 'sprint')).toBe('Phase');
    expect(resolveLabel(vocab, 'backlog')).toBe('Pending Work');
    expect(resolveLabel(vocab, 'milestone')).toBe('Inspection Point');
    expect(resolveLabel(vocab, 'release')).toBe('Release'); // unchanged
  });

  it('keeps the item types out of the vocabulary: they are the project catalog', () => {
    expect(VOCAB_KEYS).not.toContain('epic');
    expect(VOCAB_KEYS).not.toContain('task');
    expect(resolveLabel({}, 'epic')).toBe('epic');
  });
});
