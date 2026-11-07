import { describe, expect, it } from 'vitest';
import { INSTALL_CONTAINMENT, installContainment, installLine } from './install';

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

  it('shows no install line for a folder team, whatever the profile or project prepares; a repository team keeps its line', () => {
    const check = { ...git, install: { command: 'npm ci', lockfile: 'package-lock.json' } };
    expect(installLine('make deps', null, true, true)).toBeNull();
    expect(installLine(null, check, undefined, true)).toBeNull();
    expect(installLine(null, check, true, false)).toBe('Prepares with npm ci (from package-lock.json)');
  });
});

describe('installContainment', () => {
  const line = 'Prepares with npm ci (from package-lock.json)';
  const bwrap = { harness_kind: 'claude-code', sandbox: 'bwrap', sandbox_required: true } as const;
  const optedOut = { harness_kind: 'claude-code', sandbox: 'none', sandbox_required: false } as const;
  const blocked = { harness_kind: 'claude-code', sandbox: 'none', sandbox_required: true } as const;

  it('says the install is inside the sandbox where the agent is contained by bwrap', () => {
    expect(installContainment(line, [bwrap])).toBe(INSTALL_CONTAINMENT.inside);
    expect(installContainment('Replaced: make deps', [{ ...bwrap, harness_kind: 'codex' }])).toBe(INSTALL_CONTAINMENT.inside);
  });

  it('says so when the administrator turned the requirement off; the least contained computer decides', () => {
    expect(installContainment(line, [optedOut])).toBe(INSTALL_CONTAINMENT.outside);
    expect(installContainment(line, [bwrap, optedOut])).toBe(INSTALL_CONTAINMENT.outside);
  });

  it('says nothing when nothing is installed, docket installs, or the agent does not run there', () => {
    expect(installContainment(null, [bwrap])).toBeNull();
    expect(installContainment('Install turned off', [bwrap])).toBeNull();
    expect(installContainment('No lockfile: nothing is installed', [bwrap])).toBeNull();
    expect(installContainment('No install: this profile runs no commands', [bwrap])).toBeNull();
    expect(installContainment(line, [{ harness_kind: 'docket', sandbox: 'harness' }])).toBeNull();
    expect(installContainment(line, [blocked])).toBeNull();
    expect(installContainment(line, [])).toBeNull();
  });
});
