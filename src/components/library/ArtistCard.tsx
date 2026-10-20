import { useNavigate } from "react-router";
import { useT } from "../../lib/i18n";
import type { Artist } from "../../lib/subsonic/types";
import { CoverImage } from "./CoverImage";
import { ArtistContextMenu } from "./EntityContextMenu";

export function ArtistCard({ artist }: { artist: Artist }) {
  const t = useT();
  const navigate = useNavigate();
  return (
    <ArtistContextMenu artist={artist}>
      <div
        className="group w-full cursor-pointer rounded-lg p-3 transition-colors hover:bg-surface"
        onClick={() => navigate(`/artists/${artist.id}`)}
      >
        <CoverImage
          coverArt={artist.coverArt}
          size={300}
          alt={artist.name}
          className="rounded-full"
        />
        <div className="mt-2 truncate text-center text-sm font-medium">{artist.name}</div>
        <div className="truncate text-center text-xs text-dim">
          {/* Non-breaking space holds the line's height when the count is
              missing, so cards in a row stay the same height. */}
          {artist.albumCount != null ? t("count.albums", { n: artist.albumCount }) : " "}
        </div>
      </div>
    </ArtistContextMenu>
  );
}
