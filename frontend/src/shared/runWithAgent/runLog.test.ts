import { describe, it, expect } from 'vitest';
import { describeStep, parseRunLog } from './runLog';

const line = (o: unknown) => JSON.stringify(o);

describe('parseRunLog', () => {
  it('reads claude-code tool calls, their failures and the tools offered', () => {
    const log = [
      '=== stdout ===',
      line({ type: 'system', subtype: 'init', tools: ['Read', 'Write', 'Bash'] }),
      line({ type: 'assistant', message: { content: [
        { type: 'text', text: 'Reading first.' },
        { type: 'tool_use', id: 't1', name: 'Read', input: { file_path: '/repo/README.md' } },
        { type: 'tool_use', id: 't2', name: 'Bash', input: { command: 'ls', description: 'List files' } },
      ] } }),
      line({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 't2', is_error: true, content: 'boom' }] } }),
      line({ type: 'assistant', message: { content: [{ type: 'tool_use', id: 't3', name: 'Write', input: { file_path: '/repo/test.md', content: '# hi' } }] } }),
      line({ type: 'result', subtype: 'success', result: 'Done.' }),
      'not json',
      '=== stderr ===',
    ].join('\n');
    const parsed = parseRunLog(log);
    expect(parsed.toolsOffered).toEqual(['Read', 'Write', 'Bash']);
    expect(parsed.steps.map(describeStep)).toEqual(['Read /repo/README.md', 'Ran ls', 'Wrote /repo/test.md']);
    expect(parsed.steps.map((s) => s.failed)).toEqual([false, true, false]);
  });

  it('says when the agent was given no tools, and finds no steps in a log that is not stream-json', () => {
    expect(parseRunLog(line({ type: 'system', subtype: 'init', tools: [] })).toolsOffered).toEqual([]);
    expect(parseRunLog('codex output\nmore')).toEqual({ steps: [], toolsOffered: null });
  });
});
