import { Link } from "react-router";
import { artistCredits, type Credit } from "../../lib/artistCredits";

interface Props {
  /** A song (its artists) or an album (its album artists). */
  credit: Credit;
  /** Called after a link is followed — e.g. to close an overlay. */
  onNavigate?: () => void;
}

/**
 * An artist credit with one link per artist. Inline: the caller's element
 * supplies size, color and truncation.
 */
export function ArtistLinks({ credit, onNavigate }: Props) {
  const segments = artistCredits(credit);
  return (
    <>
      {segments.map((segment, i) =>
        segment.artistId ? (
          <Link
            key={i}
            to={`/artists/${segment.artistId}`}
            // Row-level click/double-click select and play the entity.
            onClick={(e) => {
              e.stopPropagation();
              onNavigate?.();
            }}
            onDoubleClick={(e) => e.stopPropagation()}
            className="hover:text-text hover:underline"
          >
            {segment.text}
          </Link>
        ) : (
          <span key={i}>{segment.text}</span>
        ),
      )}
    </>
  );
}
