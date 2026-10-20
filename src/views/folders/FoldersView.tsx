// Browse the library the way it sits on disk (getIndexes / getMusicDirectory),
// rather than by tags. Two views share this file: the tree root, and one
// directory. Songs render through the normal song table, so multi-select,
// ratings and the track context menu all come along.

import { ChevronRight, CornerLeftUp, Folder, Play, Shuffle } from "lucide-react";
import { useMemo, useRef } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import {
  ColumnsMenu,
  SongTableBody,
  SongTableHeaderRow,
} from "../../components/library/SongTable";
import { activeColumns, songGridTemplate } from "../../components/library/songColumns";
import { Button, IconButton } from "../../components/ui/Button";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { useT } from "../../lib/i18n";
import { useDirectory, useIndexes, useMusicFolders } from "../../lib/subsonic/queries";
import type { Song } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";

const selectClass =
  "cursor-pointer rounded-md border border-edge bg-surface px-2.5 py-1.5 text-sm text-text focus:border-accent focus:outline-none";

function DirectoryRow({ id, name }: { id: string; name: string }) {
  const navigate = useNavigate();
  return (
    <button
      onClick={() => navigate(`/folders/${encodeURIComponent(id)}`)}
      className="flex w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-left transition-colors hover:bg-surface"
    >
      <Folder className="size-4 shrink-0 text-faint" />
      <span className="min-w-0 flex-1 truncate text-sm">{name}</span>
      <ChevronRight className="size-4 shrink-0 text-faint" />
    </button>
  );
}

/** Shared song table for the loose files inside a directory. */
function DirectorySongs({
  songs,
  scrollRef,
}: {
  songs: Song[];
  scrollRef: React.RefObject<HTMLDivElement | null>;
}) {
  const songColumns = usePrefsStore((s) => s.prefs.songColumns);
  const columns = useMemo(() => activeColumns(songColumns), [songColumns]);
  const gridTemplate = useMemo(() => songGridTemplate(columns), [columns]);

  if (songs.length === 0) return null;
  return (
    <>
      <SongTableHeaderRow columns={columns} gridTemplate={gridTemplate} className="pt-2" />
      <div className="pt-1">
        <SongTableBody
          songs={songs}
          columns={columns}
          gridTemplate={gridTemplate}
          scrollRef={scrollRef}
        />
      </div>
    </>
  );
}

export function FoldersView() {
  const t = useT();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [params, setParams] = useSearchParams();
  const folders = useMusicFolders();
  const folderId = params.get("folder") ?? "";
  const { data, isPending, isError } = useIndexes(folderId);

  const groups = data?.index ?? [];
  const looseSongs = data?.child ?? [];
  const folderList = folders.data ?? [];

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-3 px-6 py-4">
        <h1 className="text-2xl font-semibold">{t("nav.folders")}</h1>
        {folderList.length > 1 && (
          <select
            value={folderId}
            onChange={(e) => {
              const next = new URLSearchParams(params);
              if (e.target.value === "") next.delete("folder");
              else next.set("folder", e.target.value);
              setParams(next, { replace: true });
            }}
            className={selectClass}
          >
            <option value="">{t("folders.allFolders")}</option>
            {folderList.map((folder) => (
              <option key={folder.id} value={folder.id}>
                {folder.name ?? folder.id}
              </option>
            ))}
          </select>
        )}
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        {isPending ? (
          <CenteredSpinner />
        ) : isError ? (
          <div className="p-8 text-sm text-dim">{t("folders.browseError")}</div>
        ) : groups.length === 0 && looseSongs.length === 0 ? (
          <div className="py-24 text-center text-sm text-dim">{t("folders.empty")}</div>
        ) : (
          <div className="px-4 pb-8">
            {groups.map((group) => (
              <section key={group.name}>
                <h2 className="px-3 pt-4 pb-1 text-[11px] font-semibold tracking-wide text-faint uppercase">
                  {group.name}
                </h2>
                {group.artist.map((entry) => (
                  <DirectoryRow key={entry.id} id={entry.id} name={entry.name} />
                ))}
              </section>
            ))}
            <DirectorySongs songs={looseSongs} scrollRef={scrollRef} />
          </div>
        )}
      </div>
    </div>
  );
}

export function DirectoryView() {
  const t = useT();
  const { directoryId = "" } = useParams();
  const navigate = useNavigate();
  const scrollRef = useRef<HTMLDivElement>(null);
  const playContext = usePlayerStore((s) => s.playContext);
  const { data, isPending, isError } = useDirectory(decodeURIComponent(directoryId));

  const children = data?.child ?? [];
  const subdirectories = children.filter((child) => child.isDir);
  const songs = children.filter((child) => !child.isDir);

  if (isPending) return <CenteredSpinner />;
  if (isError || !data) return <div className="p-8 text-sm text-dim">{t("folders.notFound")}</div>;

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-3 px-6 py-4">
        <IconButton
          label={t("folders.upOne")}
          onClick={() =>
            data.parent
              ? navigate(`/folders/${encodeURIComponent(data.parent)}`)
              : navigate("/folders")
          }
        >
          <CornerLeftUp className="size-4" />
        </IconButton>
        <h1 className="min-w-0 truncate text-2xl font-semibold">
          {data.name ?? t("folders.unnamed")}
        </h1>
        {songs.length > 0 && (
          <>
            <Button onClick={() => playContext(songs, 0)} className="rounded-full">
              <Play className="size-4 fill-current" /> {t("common.play")}
            </Button>
            <Button
              variant="outline"
              className="rounded-full"
              onClick={() => playContext(songs, 0, { shuffle: true })}
            >
              <Shuffle className="size-4" /> {t("common.shuffle")}
            </Button>
            <ColumnsMenu />
          </>
        )}
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        <div className="px-4 pb-8">
          {subdirectories.map((child) => (
            <DirectoryRow key={child.id} id={child.id} name={child.title} />
          ))}
          {children.length === 0 && (
            <div className="py-24 text-center text-sm text-dim">{t("folders.folderEmpty")}</div>
          )}
          <DirectorySongs songs={songs} scrollRef={scrollRef} />
        </div>
      </div>
    </div>
  );
}
