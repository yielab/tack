import type { Item } from '../types';

/**
 * Window event dispatched after the item detail drawer successfully edits an
 * item. Host views (board, list) listen for it to refresh, keeping the drawer
 * decoupled from any specific feature (no `features/* → features/*` import).
 */
export const ITEM_UPDATED_EVENT = 'tack:item-updated';

export type ItemUpdatedEvent = CustomEvent<Item>;
