import { Play } from "lucide-react";
import { useNavigate } from "react-router";
import { useT } from "../../lib/i18n";
import { cmd } from "../../lib/ipc/commands";
import type { Album } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { ArtistLinks } from "./ArtistLinks";
import { CoverImage } from "./CoverImage";
import { AlbumContextMenu } from "./EntityContextMenu";

export function AlbumCard({ album }: { album: Album }) {
  const t = useT();
  const navigate = useNavigate();

  async function playAlbum(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const full = await cmd("library_get_album", { id: album.id });
    usePlayerStore.getState().playContext(full.song, 0);
  }

  return (
    <AlbumContextMenu album={album}>
      <div
        className="group w-full cursor-pointer rounded-lg p-3 transition-colors hover:bg-surface"
        onClick={() => navigate(`/albums/${album.id}`)}
      >
        <div className="relative">
          <CoverImage coverArt={album.coverArt} size={300} alt={album.name} className="rounded-md" />
          <button
            aria-label={t("table.playSong", { title: album.name })}
            onClick={playAlbum}
            className="absolute right-2 bottom-2 flex size-10 translate-y-1 cursor-pointer items-center justify-center rounded-full bg-accent text-white opacity-0 shadow-lg transition-all group-hover:translate-y-0 group-hover:opacity-100 hover:scale-105 hover:bg-accent-bright"
          >
            <Play className="ml-0.5 size-5 fill-current" />
          </button>
        </div>
        <div className="mt-2 truncate text-sm font-medium">{album.name}</div>
        <div className="truncate text-xs text-dim">
          <ArtistLinks credit={album} />
          {album.year ? <span className="text-faint"> · {album.year}</span> : null}
        </div>
      </div>
    </AlbumContextMenu>
  );
}
