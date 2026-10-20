import { ArrowLeft, MoreVertical, Play, Shuffle } from "lucide-react";
import { useMemo, useRef } from "react";
import { useNavigate, useParams } from "react-router";
import { ArtistLinks } from "../../components/library/ArtistLinks";
import { CoverImage } from "../../components/library/CoverImage";
import { EntityGenreBubbles } from "../../components/library/GenreBubbles";
import {
  ColumnsMenu,
  SongTableBody,
  SongTableHeaderRow,
} from "../../components/library/SongTable";
import { activeColumns, songGridTemplate } from "../../components/library/songColumns";
import { EntityActionsMenu } from "../../components/library/EntityContextMenu";
import { RatingStars } from "../../components/library/RatingStars";
import { StarButton } from "../../components/library/StarButton";
import { Button, IconButton } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { formatLongDuration } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { useAlbum } from "../../lib/subsonic/queries";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";

export function AlbumDetailView() {
  const t = useT();
  const { albumId = "" } = useParams();
  const navigate = useNavigate();
  const scrollRef = useRef<HTMLDivElement>(null);
  const { data: album, isPending, isError } = useAlbum(albumId);
  const playContext = usePlayerStore((s) => s.playContext);
  const songColumns = usePrefsStore((s) => s.prefs.songColumns);

  // Every track shares this album — the Album column would be noise here.
  const columns = useMemo(() => activeColumns(songColumns, ["album"]), [songColumns]);
  const gridTemplate = useMemo(() => songGridTemplate(columns), [columns]);

  if (isPending) return <CenteredSpinner />;
  if (isError || !album) {
    return <div className="p-8 text-sm text-dim">{t("album.notFound")}</div>;
  }

  return (
    <div ref={scrollRef} className="h-full overflow-y-auto">
      <div className="flex items-end gap-6 bg-gradient-to-b from-surface to-bg px-6 pt-6 pb-6">
        <div className="relative">
          <IconButton
            label={t("common.back")}
            onClick={() => navigate(-1)}
            className="absolute -top-1 -left-1 z-10 bg-bg/60 backdrop-blur"
          >
            <ArrowLeft className="size-4" />
          </IconButton>
          <CoverImage
            coverArt={album.coverArt}
            size={600}
            alt={album.name}
            className="w-52 rounded-lg shadow-2xl"
          />
        </div>
        <div className="min-w-0 flex-1 pb-1">
          <div className="text-xs font-medium tracking-wide text-dim uppercase">
            {t("album.label")}
          </div>
          <h1 className="mt-1 mb-2 text-4xl leading-tight font-bold">{album.name}</h1>
          <div className="flex items-center gap-1.5 text-sm text-dim">
            <span className="font-medium text-text">
              <ArtistLinks credit={album} />
            </span>
            {album.year ? <span>· {album.year}</span> : null}
            <span>
              · {t("count.tracks", { n: album.songCount })}, {formatLongDuration(album.duration, t)}
            </span>
          </div>
          <EntityGenreBubbles of={album} className="mt-2.5" />
          <div className="mt-4 flex items-center gap-2">
            <Button onClick={() => playContext(album.song, 0)} className="rounded-full">
              <Play className="size-4 fill-current" /> {t("common.play")}
            </Button>
            <Button
              variant="outline"
              className="rounded-full"
              onClick={() => playContext(album.song, 0, { shuffle: true })}
            >
              <Shuffle className="size-4" /> {t("common.shuffle")}
            </Button>
            <StarButton id={album.id} kind="album" starred={album.starred} />
            <RatingStars id={album.id} rating={album.userRating} size="md" />
            <ColumnsMenu exclude={["album"]} />
            <EntityActionsMenu
              entity={{
                kind: "album",
                id: album.id,
                name: album.name,
                starred: album.starred,
              }}
              trigger={
                <button
                  aria-label={t("album.options")}
                  className="cursor-pointer rounded-full p-2 text-dim transition-colors hover:bg-surface-2 hover:text-text"
                >
                  <MoreVertical className="size-4" />
                </button>
              }
            />
          </div>
        </div>
      </div>
      <div className="pb-8">
        <SongTableHeaderRow columns={columns} gridTemplate={gridTemplate} />
        <div className="pt-1">
          <SongTableBody
            songs={album.song}
            columns={columns}
            gridTemplate={gridTemplate}
            scrollRef={scrollRef}
            showArt={false}
            useTrackNumbers
          />
        </div>
      </div>
    </div>
  );
}
