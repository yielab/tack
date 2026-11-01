import { describe, it, expect } from 'vitest';
import { truncateMiddle } from './truncateMiddle';

describe('truncateMiddle', () => {
  it('leaves a short id whole', () => {
    expect(truncateMiddle('att_1')).toBe('att_1');
    expect(truncateMiddle('dec_6174_3d6f')).toBe('dec_6174_3d6f');
  });

  it('keeps the head and the tail of a long id around an ellipsis', () => {
    expect(truncateMiddle('dec_6174aabbccdd0011223344556677_3d6f')).toBe('dec_6174…3d6f');
  });
});
