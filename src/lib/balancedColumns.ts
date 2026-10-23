/**
 * Columns for `count` items when a row has room for `fit` of them: as few
 * rows as fit, as evenly filled as they can be, so a wrapped row never leaves
 * one or two items stranded under a full one.
 */
export function balancedColumns(count: number, fit: number): number {
  if (count <= 0 || fit <= 0) return 1;
  const rows = Math.ceil(count / fit);
  return Math.ceil(count / rows);
}
