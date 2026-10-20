import { describe, expect, it } from "vitest";
import { commonGenres, genreNames } from "./GenreBubbles";
import type { Album } from "../../lib/subsonic/types";

const album = (name: string, genres: string[]): Album => ({
  id: name,
  name,
  songCount: 1,
  duration: 1,
  genres: genres.map((g) => ({ name: g })),
});

describe("genreNames", () => {
  it("uses the full list when the server sends one", () => {
    expect(
      genreNames({ genre: "Electronic", genres: [{ name: "Electronic" }, { name: "Ambient" }] }),
    ).toEqual(["Electronic", "Ambient"]);
  });

  it("falls back to the single genre on servers without the extension", () => {
    expect(genreNames({ genre: "Jazz" })).toEqual(["Jazz"]);
  });

  it("drops duplicates and blanks", () => {
    expect(
      genreNames({ genres: [{ name: "Rock" }, { name: "Rock" }, { name: " " }] }),
    ).toEqual(["Rock"]);
  });

  it("has nothing to show for an untagged entity", () => {
    expect(genreNames({})).toEqual([]);
    expect(genreNames({ genre: "" })).toEqual([]);
  });
});

describe("commonGenres", () => {
  it("ranks by how many releases carry the genre", () => {
    const albums = [
      album("a", ["Ambient", "Electronic"]),
      album("b", ["Electronic"]),
      album("c", ["Electronic", "Techno"]),
    ];
    expect(commonGenres(albums)).toEqual(["Electronic", "Ambient", "Techno"]);
  });

  it("breaks ties alphabetically so the order is stable", () => {
    expect(commonGenres([album("a", ["Zydeco", "Blues"])])).toEqual(["Blues", "Zydeco"]);
  });

  it("caps the list", () => {
    const many = album("a", ["a", "b", "c", "d", "e", "f", "g", "h"]);
    expect(commonGenres([many], 3)).toHaveLength(3);
  });

  it("returns nothing when no release is tagged", () => {
    expect(commonGenres([album("a", [])])).toEqual([]);
  });
});
