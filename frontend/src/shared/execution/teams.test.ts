import { describe, it, expect } from 'vitest';
import { isTeamOfOne, methodsByType, misfitText, notSentText, podsOnly, teamChoices, unmetNeeds, type TeamEntry, type TeamReport } from './teams';

const pod = (name: string, report: Partial<TeamReport> = {}): TeamEntry =>
  ({ name, harness: 'docket', reported: true, profiles: [], report: { name, profile: 'attended', ...report } }) as TeamEntry;
const solo = (harness: string): TeamEntry =>
  ({ name: harness, harness, reported: true, profiles: [], report: { name: harness, delivers: 'branch' } }) as TeamEntry;

describe('a team of one', () => {
  it('is_a_reported_agent_named_for_itself_with_no_supervision', () => {
    expect(isTeamOfOne(solo('claude-code'))).toBe(true);
    expect(isTeamOfOne(pod('saas-code'))).toBe(false);
    // A pod that happens to share its agent's name still has a supervision setting.
    expect(isTeamOfOne(pod('docket'))).toBe(false);
    expect(isTeamOfOne({ ...solo('codex'), reported: false })).toBe(false);
  });

  it('is_not_offered_as_a_team_a_profile_can_name', () => {
    const entries = [pod('saas-code'), solo('claude-code'), solo('codex')];
    expect(podsOnly(entries).map((e) => e.name)).toEqual(['saas-code']);
    expect(teamChoices(entries, ['old-team'])).toEqual({ reported: ['saas-code'], notReported: ['old-team'] });
  });
});

describe('misfitText', () => {
  const report = {
    name: 'fab',
    delivers: 'report',
    methods: [{ name: 'tdd', source: 's', default: true, missing_roles: [] }, { name: 'secure-build', source: 's', default: false, missing_roles: ['security-vetter'] }],
    needs: [
      { kind: 'sign_in', name: 'signed in', fix: 'Run `claude login` on the runner\'s computer.', satisfied: false },
      { kind: 'service', name: 'imap', fix: 'Declare the channel.', satisfied: false },
    ],
  } as TeamReport;

  it('keeps_the_two_original_sentences', () => {
    expect(misfitText('fab', 'no_web')).toContain('docket pod set network open --widen --pod fab');
    expect(misfitText('fab', 'no_proposals')).toContain('tack_proposals');
  });

  it('says_a_repository_run_needs_a_team_that_delivers_a_branch', () => {
    expect(misfitText('fab', 'no_repository', report)).toBe(
      "changes the project's repository, and fab hands back a report, not a branch. Point the profile at a team that delivers a branch, or make it a Researcher.",
    );
    expect(misfitText('fab', 'no_repository', { ...report, delivers: 'records' })).toContain('hands back rows in its record sets');
  });

  it('names_the_methods_a_team_has_when_a_name_is_unknown', () => {
    expect(misfitText('fab', { unknown_method: { name: 'lint' } }, report)).toBe(
      'names the method lint, which fab does not have. It has tdd, secure-build. Choose one it has, or leave the method out.',
    );
    expect(misfitText('fab', { unknown_method: { name: 'lint' } })).not.toContain('It has');
  });

  it('names_the_role_a_method_would_skip', () => {
    expect(misfitText('fab', { steps_skipped: { method: 'secure-build', roles: ['security-vetter'] } })).toContain(
      'which would skip steps on fab: no member plays security-vetter. Add that member to the team',
    );
  });

  it('gives_a_need_its_fix_and_words_a_missing_sign_in_as_one', () => {
    expect(misfitText('claude-code', { needs_unmet: { name: 'signed in' } }, report)).toBe(
      "runs claude-code, which is not signed in on the runner. Run `claude login` on the runner's computer.",
    );
    expect(misfitText('fab', { needs_unmet: { name: 'imap' } }, report)).toBe('needs imap, and fab says that is not met yet. Declare the channel.');
  });

  it('puts_not_sent_before_the_profile', () => {
    expect(notSentText('Fixer', 'fab', 'no_repository', report)).toMatch(/^Not sent: Fixer changes the project's repository/);
  });
});

describe('what a report says about the work', () => {
  it('lists_only_the_needs_not_yet_met', () => {
    const report = { name: 'x', needs: [{ kind: 'k', name: 'a', fix: 'f', satisfied: true }, { kind: 'k', name: 'b', fix: 'g', satisfied: false }] } as TeamReport;
    expect(unmetNeeds(report).map((n) => n.name)).toEqual(['b']);
    expect(unmetNeeds(undefined)).toEqual([]);
  });

  it('reads_the_method_a_profile_gives_to_each_kind_of_card', () => {
    expect(methodsByType({ docket: { team: 't', pipeline_by_type: { bug: 'tdd', feature: 'secure-build', odd: 3 } } })).toEqual([
      { type: 'bug', method: 'tdd' },
      { type: 'feature', method: 'secure-build' },
    ]);
    expect(methodsByType({})).toEqual([]);
    expect(methodsByType(null)).toEqual([]);
  });
});
