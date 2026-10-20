// Reusable pieces of the configurable-column song table, shared by the
// Songs, Album, and Playlist views: a column picker, the header row, and a
// virtualized body. Views compose them so sticky behavior stays flexible.

import { useVirtualizer } from "@tanstack/react-virtual";
import { Popover } from "radix-ui";
import { Reorder, useDragControls } from "motion/react";
import {
  ArrowDown,
  ArrowUp,
  Check,
  Columns3,
  GripVertical,
  Pause,
  Play,
} from "lucide-react";
import { useEffect, useState, type RefObject } from "react";
import { Link } from "react-router";
import { useT, type T } from "../../lib/i18n";
import type { Song } from "../../lib/subsonic/types";
import { useSelection } from "../../lib/useSelection";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { ArtistLinks } from "./ArtistLinks";
import { CoverImage } from "./CoverImage";
import { RatingStars } from "./RatingStars";
import { SelectionBar } from "./SelectionBar";
import { StarButton } from "./StarButton";
import { TrackContextMenu } from "./TrackContextMenu";
import { SONG_COLUMNS, type SongColumn, type SongColumnId } from "./songColumns";

function ShownColumnItem({
  column,
  notShownHere,
  onToggle,
}: {
  column: SongColumn;
  notShownHere: boolean;
  onToggle: () => void;
}) {
  const t = useT();
  const controls = useDragControls();
  return (
    <Reorder.Item
      value={column.id}
      dragListener={false}
      dragControls={controls}
      className="rounded bg-surface"
    >
      <div className="flex items-center gap-1.5 rounded px-1.5 py-1.5 text-sm transition-colors hover:bg-surface-2">
        <button
          aria-label={t("columns.move", { label: t(column.labelKey) })}
          onPointerDown={(e) => controls.start(e)}
          className="cursor-grab touch-none text-faint hover:text-dim active:cursor-grabbing"
        >
          <GripVertical className="size-3.5" />
        </button>
        <button
          onClick={onToggle}
          className="flex flex-1 cursor-pointer items-center gap-2 text-left"
        >
          <Check className="size-3.5 shrink-0 text-accent" />
          <span className="truncate">{t(column.labelKey)}</span>
          {notShownHere && (
            <span className="ml-auto shrink-0 text-[10px] text-faint">
              {t("columns.notHere")}
            </span>
          )}
        </button>
      </div>
    </Reorder.Item>
  );
}

/** Column picker with drag-to-reorder; selection and order are one shared
 * preference across every song table. */
export function ColumnsMenu({ exclude = [] }: { exclude?: SongColumnId[] }) {
  const t = useT();
  const songColumns = usePrefsStore((s) => s.prefs.songColumns);
  const setPrefs = usePrefsStore((s) => s.set);

  const shown = songColumns
    .map((id) => SONG_COLUMNS.find((c) => c.id === id))
    .filter((c): c is SongColumn => c != null);
  const shownIds = shown.map((c) => c.id);
  const hidden = SONG_COLUMNS.filter((c) => !shownIds.includes(c.id));

  function toggle(id: string) {
    const next = shownIds.includes(id as SongColumnId)
      ? shownIds.filter((c) => c !== id)
      : [...shownIds, id];
    setPrefs({ songColumns: next });
  }

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          title={t("columns.tooltip")}
          className="flex cursor-pointer items-center gap-1.5 rounded-md border border-edge bg-surface px-2.5 py-1.5 text-sm text-dim transition-colors hover:text-text focus:border-accent focus:outline-none"
        >
          <Columns3 className="size-4" /> {t("columns.button")}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          className="z-50 w-60 rounded-lg border border-edge bg-surface p-1 shadow-xl focus:outline-none"
        >
          <div className="px-2.5 pt-1.5 pb-1 text-[10px] font-semibold tracking-wide text-faint uppercase">
            {t("columns.shown")}
          </div>
          {shown.length === 0 && (
            <div className="px-2.5 pb-1.5 text-xs text-faint">{t("columns.noExtra")}</div>
          )}
          <Reorder.Group
            axis="y"
            values={shownIds}
            onReorder={(ids) => setPrefs({ songColumns: ids })}
          >
            {shown.map((column) => (
              <ShownColumnItem
                key={column.id}
                column={column}
                notShownHere={exclude.includes(column.id)}
                onToggle={() => toggle(column.id)}
              />
            ))}
          </Reorder.Group>
          {hidden.length > 0 && (
            <>
              <div className="mx-1.5 my-1 h-px bg-edge" />
              <div className="px-2.5 pt-0.5 pb-1 text-[10px] font-semibold tracking-wide text-faint uppercase">
                {t("columns.hidden")}
              </div>
              {hidden.map((column) => (
                <button
                  key={column.id}
                  onClick={() => toggle(column.id)}
                  className="flex w-full cursor-pointer items-center gap-2 rounded px-1.5 py-1.5 text-left text-sm text-dim transition-colors hover:bg-surface-2 hover:text-text"
                >
                  <span className="size-3.5 shrink-0" />
                  <span className="size-3.5 shrink-0" />
                  <span className="truncate">{t(column.labelKey)}</span>
                </button>
              ))}
            </>
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function HeaderCell({
  label,
  active,
  desc,
  align,
  onClick,
}: {
  label: string;
  active: boolean;
  desc: boolean;
  align?: "right";
  onClick?: () => void;
}) {
  return (
    <button
      disabled={!onClick}
      onClick={onClick}
      className={`flex items-center gap-1 truncate text-[11px] font-semibold tracking-wide uppercase select-none ${
        align === "right" ? "justify-end text-right" : "text-left"
      } ${onClick ? "cursor-pointer hover:text-text" : ""} ${active ? "text-accent" : "text-faint"}`}
    >
      {label}
      {active && (desc ? <ArrowDown className="size-3" /> : <ArrowUp className="size-3" />)}
    </button>
  );
}

interface HeaderRowProps {
  columns: SongColumn[];
  gridTemplate: string;
  sortId?: string;
  sortDesc?: boolean;
  /** When set, headers are clickable and sorting is the parent's job. */
  onSort?: (id: string) => void;
  className?: string;
}

export function SongTableHeaderRow({
  columns,
  gridTemplate,
  sortId = "",
  sortDesc = false,
  onSort,
  className = "",
}: HeaderRowProps) {
  const t = useT();
  return (
    <div
      className={`mx-4 grid items-center gap-x-3 border-b border-edge px-3 pb-2 ${className}`}
      style={{ gridTemplateColumns: gridTemplate }}
    >
      <span className="text-center text-[11px] font-semibold text-faint">#</span>
      <HeaderCell
        label={t("table.title")}
        active={sortId === "title"}
        desc={sortDesc}
        onClick={onSort ? () => onSort("title") : undefined}
      />
      {columns.map((column) => (
        <HeaderCell
          key={column.id}
          label={t(column.labelKey)}
          active={sortId === column.id}
          desc={sortDesc}
          align={column.align}
          onClick={onSort ? () => onSort(column.id) : undefined}
        />
      ))}
      <span />
    </div>
  );
}

function SongRow({
  song,
  index,
  columns,
  gridTemplate,
  showArt,
  useTrackNumbers,
  selected,
  onPlay,
  onClick,
  t,
}: {
  song: Song;
  index: number;
  columns: SongColumn[];
  gridTemplate: string;
  showArt: boolean;
  useTrackNumbers: boolean;
  selected: boolean;
  onPlay: () => void;
  onClick: (e: React.MouseEvent) => void;
  /** Passed down rather than hooked per row: thousands of rows mount here. */
  t: T;
}) {
  const isCurrent = usePlayerStore((s) => s.currentTrackId === song.id);
  const playing = usePlayerStore((s) => s.state === "playing");
  const number = useTrackNumbers ? (song.track ?? index + 1) : index + 1;

  return (
    <div
      className={`group grid h-full items-center gap-x-3 rounded-md px-3 transition-colors select-none ${
        selected ? "bg-surface-2 ring-1 ring-accent/40 ring-inset" : "hover:bg-surface"
      } ${isCurrent && !selected ? "bg-surface" : ""}`}
      style={{ gridTemplateColumns: gridTemplate }}
      onClick={onClick}
      onDoubleClick={onPlay}
    >
      <div className="flex items-center justify-center text-sm tabular-nums text-faint">
        <span className={`group-hover:hidden ${isCurrent ? "hidden" : ""}`}>{number}</span>
        <button
          aria-label={t("table.playSong", { title: song.title })}
          onClick={onPlay}
          className={`cursor-pointer ${isCurrent ? "text-accent" : "hidden text-text group-hover:block"}`}
        >
          {isCurrent && playing ? (
            <Pause className="size-4 fill-current" />
          ) : (
            <Play className="size-4 fill-current" />
          )}
        </button>
      </div>
      <div className="flex min-w-0 items-center gap-2.5">
        {showArt && (
          <CoverImage coverArt={song.coverArt} size={80} alt="" className="size-9 rounded" />
        )}
        <span className={`truncate text-sm ${isCurrent ? "font-medium text-accent" : ""}`}>
          {song.title}
        </span>
      </div>
      {columns.map((column) => {
        if (column.id === "rating") {
          return (
            <RatingStars key={column.id} id={song.id} rating={song.userRating} dimWhenUnset />
          );
        }
        const text = column.value(song, t);
        // The artist cell links each credited artist separately.
        if (column.id === "artist" && text !== "—") {
          return (
            <span key={column.id} className="truncate text-sm text-dim">
              <ArtistLinks credit={song} />
            </span>
          );
        }
        const linkTo =
          text !== "—" && column.id === "album" && song.albumId
            ? `/albums/${song.albumId}`
            : null;
        return (
          <span
            key={column.id}
            className={`truncate text-sm text-dim ${column.align === "right" ? "text-right tabular-nums" : ""}`}
          >
            {linkTo ? (
              <Link
                to={linkTo}
                onClick={(e) => e.stopPropagation()}
                onDoubleClick={(e) => e.stopPropagation()}
                className="hover:text-text hover:underline"
              >
                {text}
              </Link>
            ) : (
              text
            )}
          </span>
        );
      })}
      <div className="flex justify-end">
        <StarButton
          id={song.id}
          kind="song"
          starred={song.starred}
          className={song.starred ? "" : "opacity-0 group-hover:opacity-100"}
        />
      </div>
    </div>
  );
}

interface BodyProps {
  /** Final display order — playing a row uses this as the play context. */
  songs: Song[];
  columns: SongColumn[];
  gridTemplate: string;
  scrollRef: RefObject<HTMLDivElement | null>;
  showArt?: boolean;
  useTrackNumbers?: boolean;
  dimmed?: boolean;
  /** Enables "Remove from playlist" in the row context menu. */
  onRemoveFromPlaylist?: (songs: Song[]) => void;
}

export function SongTableBody({
  songs,
  columns,
  gridTemplate,
  scrollRef,
  showArt = true,
  useTrackNumbers = false,
  dimmed = false,
  onRemoveFromPlaylist,
}: BodyProps) {
  const t = useT();
  const playContext = usePlayerStore((s) => s.playContext);
  const selection = useSelection(songs);

  // The scroll container often mounts in the same commit as this body (the
  // views early-return a spinner while loading, so both appear together once
  // data arrives). At that point the container's ref isn't attached yet when
  // the virtualizer first looks — resolve the element in an effect so a
  // re-render with the real element always follows.
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);
  useEffect(() => {
    setScrollElement(scrollRef.current);
  });

  const virtualizer = useVirtualizer({
    count: songs.length,
    getScrollElement: () => scrollElement,
    estimateSize: () => 48,
    overscan: 12,
  });

  return (
    <>
      <SelectionBar songs={selection.selected} onClear={selection.clear} />
      <div
        className={`relative mx-4 ${dimmed ? "opacity-60" : ""}`}
        style={{ height: virtualizer.getTotalSize() }}
      >
        {virtualizer.getVirtualItems().map((vRow) => {
          const song = songs[vRow.index];
          return (
            <div
              key={vRow.key}
              className="absolute right-0 left-0"
              style={{ top: vRow.start, height: vRow.size }}
            >
              <TrackContextMenu
                song={song}
                context={songs}
                index={vRow.index}
                targets={selection.targetsFor(song)}
                onRemoveFromPlaylist={onRemoveFromPlaylist}
              >
                <div className="h-full" onContextMenu={() => selection.onRowContextMenu(vRow.index)}>
                  <SongRow
                    song={song}
                    index={vRow.index}
                    columns={columns}
                    gridTemplate={gridTemplate}
                    showArt={showArt}
                    useTrackNumbers={useTrackNumbers}
                    selected={selection.isSelected(song.id)}
                    onPlay={() => playContext(songs, vRow.index)}
                    onClick={(e) => selection.onRowClick(vRow.index, e)}
                    t={t}
                  />
                </div>
              </TrackContextMenu>
            </div>
          );
        })}
      </div>
    </>
  );
}
