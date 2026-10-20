import { Play } from "lucide-react";
import { useNavigate } from "react-router";
import { useT } from "../../lib/i18n";
import { mixCovers, mixDescription, mixTitle } from "../../lib/mixes";
import type { Mix } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { GenreMosaic } from "./GenreMosaic";

export function MixCard({ mix }: { mix: Mix }) {
  const t = useT();
  const navigate = useNavigate();
  const title = mixTitle(mix, t);

  return (
    <div
      className="group w-full cursor-pointer rounded-lg p-3 transition-colors hover:bg-surface"
      onClick={() => navigate(`/mixes/${mix.id}`)}
    >
      <div className="relative">
        <GenreMosaic covers={mixCovers(mix)} size={300} className="rounded-md" />
        <button
          aria-label={t("table.playSong", { title })}
          onClick={(e) => {
            e.stopPropagation();
            usePlayerStore.getState().playContext(mix.songs, 0);
          }}
          className="absolute right-2 bottom-2 flex size-10 translate-y-1 cursor-pointer items-center justify-center rounded-full bg-accent text-white opacity-0 shadow-lg transition-all group-hover:translate-y-0 group-hover:opacity-100 hover:scale-105 hover:bg-accent-bright"
        >
          <Play className="ml-0.5 size-5 fill-current" />
        </button>
      </div>
      <div className="mt-2 truncate text-sm font-medium">{title}</div>
      <div className="line-clamp-2 text-xs text-dim">{mixDescription(mix, t)}</div>
    </div>
  );
}
