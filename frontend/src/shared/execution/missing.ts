// What a project expects that no computer reports: the fix is the same words wherever it is shown, the
// import preview (which the server words) and the Orchestration Map (which worked it out from the
// profiles and the routes).

/** docket's command that makes the team. The shape and the recipes of a method are in a project file,
 *  which the Map has not read, so the command is the plain one. */
export const missingTeamFix = (name: string): string => `docket init ${name}`;

/** The sentence that closes a gap of an agent no computer reports. */
export const missingHarnessFix = (name: string): string =>
  `No computer reports ${name}. Install it on a computer that runs a runner, then run \`tack runner doctor\` there to check it.`;
