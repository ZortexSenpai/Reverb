import { useState } from "react";
import { artUrl, PLACEHOLDER } from "../../lib/art";

interface Props {
  coverArt?: string | null;
  size: number;
  alt: string;
  className?: string;
}

/** Album art with placeholder + fade-in, fixed aspect to avoid layout shift. */
export function CoverImage({ coverArt, size, alt, className = "" }: Props) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const src = failed ? PLACEHOLDER : artUrl(coverArt, size);

  return (
    <div className={`relative aspect-square overflow-hidden bg-surface-2 ${className}`}>
      <img
        src={src}
        alt={alt}
        loading="lazy"
        draggable={false}
        onLoad={() => setLoaded(true)}
        onError={() => setFailed(true)}
        className={`size-full object-cover transition-opacity duration-300 ${loaded ? "opacity-100" : "opacity-0"}`}
      />
    </div>
  );
}
