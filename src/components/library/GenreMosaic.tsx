import { useState } from "react";
import { artUrl, PLACEHOLDER } from "../../lib/art";

/** Covers needed to fill the 2×2 tile; fewer means a single cover instead. */
export const MOSAIC_SIZE = 4;

function Quadrant({ coverArt, size }: { coverArt?: string | null; size: number }) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  return (
    <img
      src={failed ? PLACEHOLDER : artUrl(coverArt, size)}
      alt=""
      loading="lazy"
      draggable={false}
      onLoad={() => setLoaded(true)}
      onError={() => setFailed(true)}
      className={`size-full object-cover transition-opacity duration-300 ${
        loaded ? "opacity-100" : "opacity-0"
      }`}
    />
  );
}

interface Props {
  /** Cover-art ids, most representative first. */
  covers: (string | undefined)[];
  /** Requested pixel size per image. */
  size: number;
  className?: string;
}

/** Square collage standing in for a genre, which has no artwork of its own. */
export function GenreMosaic({ covers, size, className = "" }: Props) {
  return (
    <div className={`relative aspect-square overflow-hidden bg-surface-2 ${className}`}>
      {covers.length >= MOSAIC_SIZE ? (
        <div className="grid size-full grid-cols-2 grid-rows-2">
          {covers.slice(0, MOSAIC_SIZE).map((coverArt, i) => (
            <Quadrant key={`${coverArt}-${i}`} coverArt={coverArt} size={size} />
          ))}
        </div>
      ) : (
        <Quadrant coverArt={covers[0]} size={size} />
      )}
    </div>
  );
}
