import { describe, it, expect } from 'vitest';
import { formatBudgetCap, suggestRemoteProjectName, isFullPodShape } from './format';

describe('formatBudgetCap', () => {
  it('reports "no budget cap" distinctly from a real, currency-formatted value', () => {
    expect(formatBudgetCap(null)).toBe('no budget cap');
    expect(formatBudgetCap(undefined)).toBe('no budget cap');
    expect(formatBudgetCap(0)).not.toBe('no budget cap');
    expect(formatBudgetCap(25)).toContain('25');
    expect(formatBudgetCap(25)).toMatch(/\$/);
  });

  it('never contains the word "estimated" — a budget cap is not a spend estimate', () => {
    expect(formatBudgetCap(50)).not.toMatch(/estimated/i);
  });
});

describe('suggestRemoteProjectName', () => {
  it('slugifies a human project name', () => {
    expect(suggestRemoteProjectName('Blog API')).toBe('blog-api');
    expect(suggestRemoteProjectName('  Weird!!  Name_2 ')).toBe('weird-name-2');
  });

  it('falls back to a non-empty default when the name has no usable characters', () => {
    expect(suggestRemoteProjectName('')).toBe('new-project');
    expect(suggestRemoteProjectName('!!!')).toBe('new-project');
  });
});

describe('isFullPodShape', () => {
  it('matches only the literal "full", case-insensitively, mirroring the Rust handler', () => {
    expect(isFullPodShape('full')).toBe(true);
    expect(isFullPodShape('Full')).toBe(true);
    expect(isFullPodShape('FULL')).toBe(true);
    expect(isFullPodShape('partial')).toBe(false);
    expect(isFullPodShape(null)).toBe(false);
    expect(isFullPodShape(undefined)).toBe(false);
    expect(isFullPodShape('')).toBe(false);
  });
});
