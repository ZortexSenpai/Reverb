import { Pause, Play } from "lucide-react";
import type { Song } from "../../lib/subsonic/types";
import { formatDuration } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { usePlayerStore } from "../../stores/playerStore";
import { ArtistLinks } from "./ArtistLinks";
import { CoverImage } from "./CoverImage";
import { StarButton } from "./StarButton";

interface Props {
  song: Song;
  index: number;
  onPlay: () => void;
  showArt?: boolean;
  showArtist?: boolean;
  selected?: boolean;
  onClick?: (e: React.MouseEvent) => void;
}

export function TrackRow({
  song,
  index,
  onPlay,
  showArt = false,
  showArtist = true,
  selected = false,
  onClick,
}: Props) {
  const t = useT();
  const isCurrent = usePlayerStore((s) => s.currentTrackId === song.id);
  const playing = usePlayerStore((s) => s.state === "playing");

  return (
    <div
      className={`group flex cursor-pointer items-center gap-3 rounded-md px-3 py-2 transition-colors select-none ${
        selected ? "bg-surface-2 ring-1 ring-accent/40 ring-inset" : "hover:bg-surface"
      } ${isCurrent && !selected ? "bg-surface" : ""}`}
      onClick={onClick}
      onDoubleClick={onPlay}
    >
      <div className="flex w-8 shrink-0 items-center justify-center text-sm tabular-nums text-faint">
        <span className={`group-hover:hidden ${isCurrent ? "hidden" : ""}`}>
          {song.track ?? index + 1}
        </span>
        <button
          aria-label={t("table.playSong", { title: song.title })}
          onClick={onPlay}
          className={`cursor-pointer text-text ${isCurrent ? "" : "hidden group-hover:block"} ${
            isCurrent ? "text-accent" : ""
          }`}
        >
          {isCurrent && playing ? (
            <Pause className="size-4 fill-current" />
          ) : (
            <Play className="size-4 fill-current" />
          )}
        </button>
      </div>
      {showArt && (
        <CoverImage coverArt={song.coverArt} size={80} alt="" className="size-10 rounded" />
      )}
      <div className="min-w-0 flex-1">
        <div className={`truncate text-sm ${isCurrent ? "font-medium text-accent" : ""}`}>
          {song.title}
        </div>
        {showArtist && song.artist ? (
          <div className="truncate text-xs text-dim">
            <ArtistLinks credit={song} />
          </div>
        ) : null}
      </div>
      <StarButton
        id={song.id}
        kind="song"
        starred={song.starred}
        className={song.starred ? "" : "opacity-0 group-hover:opacity-100"}
      />
      <div className="w-12 text-right text-xs tabular-nums text-dim">
        {formatDuration(song.duration)}
      </div>
    </div>
  );
}
