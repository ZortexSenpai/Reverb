import { Heart } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useStarMutation } from "../../lib/subsonic/queries";
import type { StarKind } from "../../lib/subsonic/types";

interface Props {
  id: string;
  kind: StarKind;
  starred?: string;
  className?: string;
}

export function StarButton({ id, kind, starred, className = "" }: Props) {
  const t = useT();
  const setStar = useStarMutation();
  const isStarred = Boolean(starred);
  const label = isStarred ? t("favourite.remove") : t("favourite.add");
  return (
    <button
      aria-label={label}
      title={label}
      onClick={(e) => {
        e.stopPropagation();
        void setStar(id, kind, !isStarred);
      }}
      className={`cursor-pointer rounded-full p-1.5 transition-colors ${
        isStarred ? "text-accent hover:text-accent-bright" : "text-dim hover:text-text"
      } ${className}`}
    >
      <Heart className={`size-4 ${isStarred ? "fill-current" : ""}`} />
    </button>
  );
}
