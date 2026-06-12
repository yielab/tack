import { describe, it, expect } from 'vitest';
import { resolveLabel, getItemTypeList, getItemTypeMap, ITEM_TYPE_KEYS } from './vocab';

describe('resolveLabel', () => {
  it('returns the vocab override when the key is present', () => {
    expect(resolveLabel({ task: 'Work Order' }, 'task')).toBe('Work Order');
  });

  it('falls back to the built-in default when the key is not in vocab', () => {
    expect(resolveLabel({}, 'task')).toBe('Task');
    expect(resolveLabel(undefined, 'sprint')).toBe('Sprint');
  });

  it('returns the key itself for an unknown vocab key', () => {
    expect(resolveLabel({}, 'unknown_key')).toBe('unknown_key');
  });

  it('handles undefined vocab gracefully', () => {
    expect(resolveLabel(undefined, 'epic')).toBe('Epic');
  });

  it('applies construction-style overrides across multiple keys', () => {
    const vocab = { task: 'Work Order', sprint: 'Phase', epic: 'Building' };
    expect(resolveLabel(vocab, 'task')).toBe('Work Order');
    expect(resolveLabel(vocab, 'sprint')).toBe('Phase');
    expect(resolveLabel(vocab, 'epic')).toBe('Building');
    expect(resolveLabel(vocab, 'bug')).toBe('Bug'); // unchanged
  });
});

describe('getItemTypeList', () => {
  it('returns one entry per ITEM_TYPE_KEYS key', () => {
    const list = getItemTypeList();
    expect(list).toHaveLength(ITEM_TYPE_KEYS.length);
  });

  it('each entry has value, label, emoji, and color', () => {
    for (const entry of getItemTypeList()) {
      expect(typeof entry.value).toBe('string');
      expect(typeof entry.label).toBe('string');
      expect(typeof entry.emoji).toBe('string');
      expect(typeof entry.color).toBe('string');
    }
  });

  it('applies vocab overrides to labels', () => {
    const list = getItemTypeList({ task: 'Ticket' });
    const task = list.find((e) => e.value === 'task')!;
    expect(task.label).toBe('Ticket');
  });

  it('uses default label when no vocab override', () => {
    const list = getItemTypeList();
    const bug = list.find((e) => e.value === 'bug')!;
    expect(bug.label).toBe('Bug');
  });
});

describe('getItemTypeMap', () => {
  it('has an entry for every ITEM_TYPE_KEYS key', () => {
    const map = getItemTypeMap();
    for (const key of ITEM_TYPE_KEYS) {
      expect(map[key]).toBeDefined();
    }
  });

  it('applies vocab overrides to map labels', () => {
    const map = getItemTypeMap({ epic: 'Initiative' });
    expect(map['epic'].label).toBe('Initiative');
    expect(map['task'].label).toBe('Task'); // unaffected default
  });

  it('each map entry has label, color, and emoji', () => {
    const map = getItemTypeMap();
    for (const entry of Object.values(map)) {
      expect(typeof entry.label).toBe('string');
      expect(typeof entry.color).toBe('string');
      expect(typeof entry.emoji).toBe('string');
    }
  });
});
