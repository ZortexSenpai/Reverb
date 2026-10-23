import { useState } from "react";
import { artUrl, PLACEHOLDER } from "../../lib/art";

interface Props {
  coverArt?: string | null;
  size: number;
  alt: string;
  className?: string;
  /** Shown in place of the vinyl placeholder when there's no art or it fails to load. */
  fallback?: React.ReactNode;
}

/** Album art with placeholder + fade-in, fixed aspect to avoid layout shift. */
export function CoverImage({ coverArt, size, alt, className = "", fallback }: Props) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const src = failed ? PLACEHOLDER : artUrl(coverArt, size);
  const missing = !coverArt || failed;

  return (
    <div className={`relative aspect-square overflow-hidden bg-surface-2 ${className}`}>
      {fallback && missing ? (
        fallback
      ) : (
        <img
          src={src}
          alt={alt}
          loading="lazy"
          draggable={false}
          onLoad={() => setLoaded(true)}
          onError={() => setFailed(true)}
          className={`size-full object-cover transition-opacity duration-300 ${loaded ? "opacity-100" : "opacity-0"}`}
        />
      )}
    </div>
  );
}
