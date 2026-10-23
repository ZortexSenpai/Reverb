// What an artist, album or Song on the stats page does when used, the way its
// counterpart does in the library views. Only a Library match leads anywhere:
// an entry without one has no page, nothing to play and no context menu.

import type { StatsItem } from "./subsonic/types";

export type StatsEntryKind = "artist" | "album" | "song";

/**
 * A matched entry: the library item behind it, for its context menu, and the
 * page a click opens. A Song has no page; double-clicking plays it.
 */
export type StatsEntryAction =
  | { kind: "artist" | "album"; id: string; page: string }
  | { kind: "song"; id: string };

export function statsEntryAction(kind: StatsEntryKind, item: StatsItem): StatsEntryAction | null {
  const id = item.id;
  if (!id) return null;
  switch (kind) {
    case "artist":
      return { kind, id, page: `/artists/${id}` };
    case "album":
      return { kind, id, page: `/albums/${id}` };
    case "song":
      return { kind, id };
  }
}
