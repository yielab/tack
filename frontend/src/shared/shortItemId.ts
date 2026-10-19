/** The six characters an item is known by on screen: the head of its id, upper-cased. */
export const shortItemId = (id: string): string => id.replace(/-/g, '').slice(0, 6).toUpperCase();
