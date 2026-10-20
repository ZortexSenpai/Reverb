// 1–5 star rating (Subsonic setRating). Clicking the current value clears it,
// which is how every Subsonic client treats a second click on the same star.

import { Star } from "lucide-react";
import { useState } from "react";
import { useT } from "../../lib/i18n";
import { useRatingMutation } from "../../lib/subsonic/queries";

interface Props {
  id: string;
  rating?: number;
  /** Star size in px-ish tailwind steps: 3.5 for table rows, 4 for headers. */
  size?: "sm" | "md";
  /** Hide unset stars until hover (table rows stay quiet when unrated). */
  dimWhenUnset?: boolean;
  className?: string;
}

export function RatingStars({
  id,
  rating = 0,
  size = "sm",
  dimWhenUnset = false,
  className = "",
}: Props) {
  const t = useT();
  const setRating = useRatingMutation();
  const [hovered, setHovered] = useState(0);
  const shown = hovered || rating;
  const starSize = size === "sm" ? "size-3.5" : "size-4";

  return (
    <div
      onMouseLeave={() => setHovered(0)}
      className={`flex items-center ${
        dimWhenUnset && rating === 0 ? "opacity-0 group-hover:opacity-100" : ""
      } ${className}`}
    >
      {[1, 2, 3, 4, 5].map((value) => (
        <button
          key={value}
          aria-label={t("rating.setStars", { n: value })}
          title={rating === value ? t("rating.clear") : t("rating.set", { n: value })}
          onMouseEnter={() => setHovered(value)}
          onClick={(e) => {
            e.stopPropagation();
            void setRating(id, rating === value ? 0 : value);
          }}
          onDoubleClick={(e) => e.stopPropagation()}
          className="cursor-pointer px-px py-1 text-dim transition-colors hover:text-accent"
        >
          <Star
            className={`${starSize} ${
              value <= shown ? "fill-accent text-accent" : "text-faint"
            }`}
          />
        </button>
      ))}
    </div>
  );
}
