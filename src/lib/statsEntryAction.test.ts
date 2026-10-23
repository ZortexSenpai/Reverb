import { describe, expect, it } from "vitest";
import { statsEntryAction } from "./statsEntryAction";

describe("statsEntryAction", () => {
  it("opens a matched artist's page", () => {
    expect(statsEntryAction("artist", { name: "Radiohead", id: "ar-1" })).toEqual({
      kind: "artist",
      id: "ar-1",
      page: "/artists/ar-1",
    });
  });

  it("opens a matched album's page", () => {
    expect(statsEntryAction("album", { name: "OK Computer", artist: "Radiohead", id: "al-1" })).toEqual({
      kind: "album",
      id: "al-1",
      page: "/albums/al-1",
    });
  });

  it("plays a matched Song, which has no page of its own", () => {
    expect(statsEntryAction("song", { name: "Airbag", artist: "Radiohead", id: "s-1" })).toEqual({
      kind: "song",
      id: "s-1",
    });
  });

  it("does nothing for an entry without a Library match", () => {
    // The summary sends `null` for an unmatched entry; a missing id is the same.
    for (const kind of ["artist", "album", "song"] as const) {
      expect(statsEntryAction(kind, { name: "Unknown", artist: "Nobody", id: null })).toBeNull();
      expect(statsEntryAction(kind, { name: "Unknown" })).toBeNull();
    }
  });
});
