import { Pin, Play } from "lucide-react";
import { useNavigate } from "react-router";
import { formatLongDuration } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { cmd } from "../../lib/ipc/commands";
import type { Playlist } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { CoverImage } from "./CoverImage";

export function PlaylistCard({ playlist }: { playlist: Playlist }) {
  const t = useT();
  const navigate = useNavigate();
  const isPinned = usePrefsStore((s) => s.prefs.pinnedPlaylists.includes(playlist.id));
  const togglePinned = usePrefsStore((s) => s.togglePinnedPlaylist);

  // The list response carries no tracks, so playing means fetching the
  // playlist first — same trade as AlbumCard.
  async function playPlaylist(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const full = await cmd("library_get_playlist", { id: playlist.id });
    if (full.entry.length > 0) usePlayerStore.getState().playContext(full.entry, 0);
  }

  return (
    <div
      className="group w-full cursor-pointer rounded-lg p-3 transition-colors hover:bg-surface"
      onClick={() => navigate(`/playlists/${playlist.id}`)}
    >
      <div className="relative">
        <CoverImage
          coverArt={playlist.coverArt}
          size={300}
          alt={playlist.name}
          className="rounded-md"
        />
        <button
          aria-label={isPinned ? t("playlist.unpin") : t("playlist.pin")}
          title={isPinned ? t("playlist.unpinFromSidebar") : t("playlist.pinToSidebar")}
          onClick={(e) => {
            e.stopPropagation();
            togglePinned(playlist.id);
          }}
          className={`absolute top-2 left-2 flex size-8 cursor-pointer items-center justify-center rounded-full bg-bg/60 backdrop-blur transition-all hover:bg-bg/80 ${
            isPinned
              ? "text-accent hover:text-accent-bright"
              : "text-white opacity-0 group-hover:opacity-100"
          }`}
        >
          <Pin className={`size-4 ${isPinned ? "fill-current" : ""}`} />
        </button>
        <button
          aria-label={t("table.playSong", { title: playlist.name })}
          onClick={playPlaylist}
          className="absolute right-2 bottom-2 flex size-10 translate-y-1 cursor-pointer items-center justify-center rounded-full bg-accent text-white opacity-0 shadow-lg transition-all group-hover:translate-y-0 group-hover:opacity-100 hover:scale-105 hover:bg-accent-bright"
        >
          <Play className="ml-0.5 size-5 fill-current" />
        </button>
      </div>
      <div className="mt-2 truncate text-sm font-medium">{playlist.name}</div>
      <div className="truncate text-xs text-dim">
        {t("count.tracks", { n: playlist.songCount })} · {formatLongDuration(playlist.duration, t)}
        {playlist.owner ? <span className="text-faint"> · {playlist.owner}</span> : null}
      </div>
    </div>
  );
}
