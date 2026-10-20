import { describe, expect, it } from "vitest";
import { artistCredits } from "./artistCredits";

/** Compact view of a credit: linked segments as "name→id", literals as-is. */
const shape = (song: Parameters<typeof artistCredits>[0]) =>
  artistCredits(song).map((s) => (s.artistId ? `${s.text}→${s.artistId}` : s.text));

describe("artistCredits", () => {
  it("links each artist in the credit and keeps the separator intact", () => {
    expect(
      shape({
        artist: "Alpha Artist feat. Epsilon Artist",
        artistId: "a-alpha",
        artists: [
          { id: "a-alpha", name: "Alpha Artist" },
          { id: "a-eps", name: "Epsilon Artist" },
        ],
      }),
    ).toEqual(["Alpha Artist→a-alpha", " feat. ", "Epsilon Artist→a-eps"]);
  });

  it("prefers displayArtist over the legacy artist string", () => {
    expect(
      shape({
        artist: "Alpha Artist",
        displayArtist: "Alpha Artist / Gamma Artist",
        artists: [
          { id: "a-alpha", name: "Alpha Artist" },
          { id: "a-gamma", name: "Gamma Artist" },
        ],
      }),
    ).toEqual(["Alpha Artist→a-alpha", " / ", "Gamma Artist→a-gamma"]);
  });

  it("does not let a shorter name shadow a longer one at the same position", () => {
    expect(
      shape({
        artist: "Bob Marley & Bob",
        artists: [
          { id: "bob", name: "Bob" },
          { id: "marley", name: "Bob Marley" },
        ],
      }),
    ).toEqual(["Bob Marley→marley", " & ", "Bob→bob"]);
  });

  it("falls back to a plain list when the credit omits an artist", () => {
    expect(
      shape({
        artist: "Alpha Artist",
        artistId: "a-alpha",
        artists: [
          { id: "a-alpha", name: "Alpha Artist" },
          { id: "a-delta", name: "Delta Artist" },
        ],
      }),
    ).toEqual(["Alpha Artist→a-alpha", ", ", "Delta Artist→a-delta"]);
  });

  it("links the whole string for a single artist", () => {
    expect(shape({ artist: "Alpha Artist", artistId: "a-alpha" })).toEqual([
      "Alpha Artist→a-alpha",
    ]);
  });

  it("keeps a credit the server cannot link as plain text", () => {
    expect(shape({ artist: "Alpha Artist;Beta Artist" })).toEqual(["Alpha Artist;Beta Artist"]);
    expect(shape({ artists: [{ id: "", name: "" }], artist: "Unknown" })).toEqual(["Unknown"]);
  });

  it("has nothing to render without a credit", () => {
    expect(artistCredits({})).toEqual([]);
    expect(artistCredits({ artistId: "a-alpha" })).toEqual([]);
  });
});
