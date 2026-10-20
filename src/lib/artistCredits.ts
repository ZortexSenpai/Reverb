// An artist credit — a song's artists, or an album's album artists — is one
// string ("A feat. B") plus a list of the artists in it. Servers also send
// `artistId`, but that is only the *first* artist, so linking the whole string
// with it sends every click to that one artist. The string is instead split
// into the artist names inside it and the literal separators between them,
// giving each artist its own link while keeping the server's formatting.

import type { Album, ArtistRef, Song } from "./subsonic/types";

export interface CreditSegment {
  text: string;
  /** Set when this segment names an artist that can be linked. */
  artistId?: string;
}

/** The credit fields, carried alike by songs and albums. */
export type Credit = Pick<Song | Album, "artist" | "artistId" | "artists" | "displayArtist">;

export function artistCredits(credit: Credit): CreditSegment[] {
  const display = credit.displayArtist || credit.artist || "";
  const artists = (credit.artists ?? []).filter((a) => a.id && a.name);

  // Servers without the OpenSubsonic extension — and songs restored from a
  // queue snapshot written before it — carry a single artist at most.
  if (artists.length === 0) {
    if (!display) return [];
    return [credit.artistId ? { text: display, artistId: credit.artistId } : { text: display }];
  }
  // Every artist has to stay reachable: if the credit string doesn't spell
  // them out, list them instead of linking whatever happens to match.
  return splitCredit(display, artists) ?? joinNames(artists);
}

/** The artists a credit can navigate to, in credit order. */
export function linkableArtists(credit: Credit): ArtistRef[] {
  return artistCredits(credit).flatMap((segment) =>
    segment.artistId ? [{ id: segment.artistId, name: segment.text }] : [],
  );
}

/** Segments of `display`, or null unless every artist was found in it. */
function splitCredit(display: string, artists: ArtistRef[]): CreditSegment[] | null {
  const pending = artists.map((a) => ({ ...a, used: false }));
  const segments: CreditSegment[] = [];
  let literal = "";
  let i = 0;

  while (i < display.length) {
    // Longest match wins, so "Bob" can't shadow "Bob Marley" at the same spot.
    const match = pending
      .filter((a) => !a.used && display.startsWith(a.name, i))
      .sort((a, b) => b.name.length - a.name.length)[0];
    if (!match) {
      literal += display[i];
      i += 1;
      continue;
    }
    if (literal) {
      segments.push({ text: literal });
      literal = "";
    }
    segments.push({ text: match.name, artistId: match.id });
    match.used = true;
    i += match.name.length;
  }
  if (literal) segments.push({ text: literal });

  return pending.every((a) => a.used) ? segments : null;
}

function joinNames(artists: ArtistRef[]): CreditSegment[] {
  return artists.flatMap((artist, i) => {
    const link = { text: artist.name, artistId: artist.id };
    return i === 0 ? [link] : [{ text: ", " }, link];
  });
}
