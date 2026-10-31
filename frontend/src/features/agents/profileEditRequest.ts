import { createSignal } from 'solid-js';

/** The profile the Agents page should open for editing, set by whatever just made it (a team's
 *  "Make a profile for this team") and cleared by the profile list once it has opened the form. */
export const [profileEditRequest, requestProfileEdit] = createSignal<string | null>(null);
