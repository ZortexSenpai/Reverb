import { ArrowLeft, ListPlus, Play, Shuffle } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { GenreMosaic } from "../../components/library/GenreMosaic";
import {
  ColumnsMenu,
  SongTableBody,
  SongTableHeaderRow,
} from "../../components/library/SongTable";
import {
  activeColumns,
  compareSongsBy,
  songGridTemplate,
} from "../../components/library/songColumns";
import { Button, IconButton } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { formatLongDuration } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { mixCovers, mixDescription, mixTitle } from "../../lib/mixes";
import { useMixes } from "../../lib/subsonic/queries";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";

export function MixDetailView() {
  const t = useT();
  const { mixId = "" } = useParams();
  const navigate = useNavigate();
  const scrollRef = useRef<HTMLDivElement>(null);
  const { data: mixes, isPending } = useMixes();
  const playContext = usePlayerStore((s) => s.playContext);
  const songColumns = usePrefsStore((s) => s.prefs.songColumns);
  const openNewPlaylist = useUiStore((s) => s.openNewPlaylist);

  const [sortId, setSortId] = useState("");
  const [sortDesc, setSortDesc] = useState(false);

  const mix = mixes?.find((m) => m.id === mixId);
  const columns = useMemo(() => activeColumns(songColumns), [songColumns]);
  const gridTemplate = useMemo(() => songGridTemplate(columns), [columns]);

  // Sorting is a client-side display order; default is mix order.
  const songs = useMemo(() => {
    const raw = mix?.songs ?? [];
    const compare = sortId ? compareSongsBy(sortId) : null;
    if (!compare) return raw;
    const sorted = [...raw].sort(compare);
    return sortDesc ? sorted.reverse() : sorted;
  }, [mix?.songs, sortId, sortDesc]);

  function cycleSort(id: string) {
    if (sortId !== id) {
      setSortId(id);
      setSortDesc(false);
    } else if (!sortDesc) {
      setSortDesc(true);
    } else {
      setSortId("");
      setSortDesc(false);
    }
  }

  if (isPending) return <CenteredSpinner />;
  if (!mix) {
    return <div className="p-8 text-sm text-dim">{t("mix.notFound")}</div>;
  }

  const title = mixTitle(mix, t);
  const duration = mix.songs.reduce((sum, song) => sum + (song.duration ?? 0), 0);

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
          <GenreMosaic
            covers={mixCovers(mix)}
            size={300}
            className="w-44 rounded-lg shadow-2xl"
          />
        </div>
        <div className="min-w-0 flex-1 pb-1">
          <div className="text-xs font-medium tracking-wide text-dim uppercase">
            {t("mix.label")}
            {mix.genre ? ` · ${mix.genre}` : ""}
          </div>
          <h1 className="mt-1 mb-2 text-4xl leading-tight font-bold">{title}</h1>
          <p className="text-sm text-dim">{mixDescription(mix, t)}</p>
          <div className="mt-1 text-sm text-dim">
            {t("count.tracks", { n: mix.songs.length })} · {formatLongDuration(duration, t)}
          </div>
          <div className="mt-4 flex items-center gap-2">
            <Button
              onClick={() => playContext(songs, 0)}
              disabled={songs.length === 0}
              className="rounded-full"
            >
              <Play className="size-4 fill-current" /> {t("common.play")}
            </Button>
            <Button
              variant="outline"
              className="rounded-full"
              disabled={songs.length === 0}
              onClick={() => playContext(songs, 0, { shuffle: true })}
            >
              <Shuffle className="size-4" /> {t("common.shuffle")}
            </Button>
            <IconButton
              label={t("mix.saveAsPlaylist")}
              onClick={() => openNewPlaylist(mix.songs.map((song) => song.id), title)}
            >
              <ListPlus className="size-4" />
            </IconButton>
            <ColumnsMenu />
          </div>
        </div>
      </div>
      <div className="pt-2">
        <SongTableHeaderRow
          columns={columns}
          gridTemplate={gridTemplate}
          sortId={sortId}
          sortDesc={sortDesc}
          onSort={cycleSort}
        />
      </div>
      <div className="pb-8">
        <SongTableBody
          songs={songs}
          columns={columns}
          gridTemplate={gridTemplate}
          scrollRef={scrollRef}
        />
      </div>
    </div>
  );
}
