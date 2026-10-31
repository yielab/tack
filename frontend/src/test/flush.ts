/** Lets the pending timers and promise callbacks run, so a test sees what a mounted view rendered after its fetches settled. */
export const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
