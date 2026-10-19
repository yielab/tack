import { describe, expect, it } from 'vitest';
import { installLine } from './install';

const git = { exists: true, is_dir: true, is_git: true, branch: 'main', remote_url: null, dirty_files: 0 };

describe('installLine', () => {
  it('names the detected command and its lockfile', () => {
    const check = { ...git, install: { command: 'npm ci', lockfile: 'package-lock.json' } };
    expect(installLine(null, check)).toBe('Prepares with npm ci (from package-lock.json)');
  });

  it('says so when a readable repository has no lockfile', () => {
    expect(installLine(null, { ...git, install: null })).toBe('No lockfile: nothing is installed');
  });

  it('leaves detection to the runner when the board cannot read the repository', () => {
    const later = 'Detected on the runner when the run starts';
    expect(installLine(undefined, null)).toBe(later);
    expect(installLine(null, { ...git, is_git: false })).toBe(later);
  });

  it('shows a replacement and an install turned off whatever the lockfile says', () => {
    const check = { ...git, install: { command: 'npm ci', lockfile: 'package-lock.json' } };
    expect(installLine('make deps', check)).toBe('Replaced: make deps');
    expect(installLine('', check)).toBe('Install turned off');
  });

  it('says a profile that runs no commands installs nothing, whatever the project prepares', () => {
    expect(installLine('make deps', null, false)).toBe('No install: this profile runs no commands');
    expect(installLine('make deps', null, true)).toBe('Replaced: make deps');
  });
});
