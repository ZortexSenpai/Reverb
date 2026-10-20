import { Link } from "react-router";
import { genrePath } from "../../lib/subsonic/genres";
import type { Album, Song } from "../../lib/subsonic/types";

/** Anything the server tags with genres. */
type Tagged = Pick<Song | Album, "genre" | "genres">;

/**
 * Every genre on an entity, de-duplicated, in tag order. `genre` holds only the
 * first one, so it is a fallback for servers without the OpenSubsonic array.
 */
export function genreNames(tagged: Tagged): string[] {
  const names = (tagged.genres ?? []).map((g) => g.name);
  const all = names.length > 0 ? names : tagged.genre ? [tagged.genre] : [];
  return [...new Set(all.filter((name) => name.trim() !== ""))];
}

/**
 * The genres of a set of albums, most common first — an artist has no genres of
 * its own on the wire, so its releases stand in for them.
 */
export function commonGenres(albums: Album[], limit = 6): string[] {
  const counts = new Map<string, number>();
  for (const album of albums) {
    for (const name of genreNames(album)) {
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, limit)
    .map(([name]) => name);
}

interface Props {
  names: string[];
  /** Called after a bubble is followed — e.g. to close an overlay. */
  onNavigate?: () => void;
  className?: string;
}

/** Genres as chips, each linking to that genre's view. */
export function GenreBubbles({ names, onNavigate, className = "" }: Props) {
  if (names.length === 0) return null;
  return (
    <div className={`flex flex-wrap items-center gap-1.5 ${className}`}>
      {names.map((name) => (
        <Link
          key={name}
          to={genrePath(name)}
          onClick={(e) => {
            e.stopPropagation();
            onNavigate?.();
          }}
          className="rounded-full border border-edge bg-surface-2/60 px-2.5 py-0.5 text-xs text-dim transition-colors hover:border-accent/60 hover:bg-surface-2 hover:text-text"
        >
          {name}
        </Link>
      ))}
    </div>
  );
}

/** Genre names on an entity, ready for [`GenreBubbles`]. */
export function EntityGenreBubbles({
  of,
  ...rest
}: { of: Tagged } & Omit<Props, "names">) {
  return <GenreBubbles names={genreNames(of)} {...rest} />;
}
