const FIRST_LETTER = /[\p{L}\p{N}]/u;

/**
 * Up to two letters standing for a name in a monogram: the first letter or
 * digit of its first and last words. "?" when the name has none.
 */
export function initials(name: string): string {
  const letters = name
    .split(/\s+/)
    .map((word) => word.match(FIRST_LETTER)?.[0])
    .filter((letter) => letter !== undefined);
  if (letters.length === 0) return "?";
  const picked = letters.length === 1 ? letters : [letters[0], letters[letters.length - 1]];
  return picked.join("").toUpperCase();
}
