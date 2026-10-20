// Genre names are free-text tags and routinely contain "/", "&" and "#"
// (e.g. "Hip-Hop/Rap"), so they are always fully percent-encoded into the
// single :genre path segment and decoded back by useParams.

export function genrePath(genre: string): string {
  return `/genres/${encodeURIComponent(genre)}`;
}
