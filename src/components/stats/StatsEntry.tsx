// The one way the stats page shows an artist, album or Song: its library art
// (or a placeholder), its name and how often it was played. Every section
// that lists entries uses it, so they all look and behave alike.

import { CoverImage } from "../library/CoverImage";
import { useT } from "../../lib/i18n";
import { initials } from "../../lib/initials";
import type { StatsEntryKind } from "../../lib/statsEntryAction";
import type { StatsItem } from "../../lib/subsonic/types";
import { StatsEntryActions } from "./StatsEntryActions";

export type { StatsEntryKind };

/** A matched entry lights up under the pointer, like a library card or row. */
const HOVER = "transition-colors hover:bg-surface-2";

/** Art sizes already in the cover cache: hero, card and row-sized. */
const HERO_ART = 600;
const TILE_ART = 300;
const ROW_ART = 80;

/** A stable React key: the library item, else its names. */
export function statsEntryKey(item: StatsItem): string {
  return item.id ?? `${item.name}\u0001${item.artist ?? ""}`;
}

const MONOGRAM_FILL = "color-mix(in oklab, var(--color-accent) 18%, var(--color-surface-2))";

/** Accent-tinted initials for an artist without art. Scales with its box. */
function Monogram({ name }: { name: string }) {
  return (
    <svg
      viewBox="0 0 100 100"
      aria-hidden
      className="size-full font-semibold text-accent select-none"
      style={{ background: MONOGRAM_FILL }}
    >
      <text
        x="50"
        y="50"
        textAnchor="middle"
        dominantBaseline="central"
        fontSize="38"
        fill="currentColor"
      >
        {initials(name)}
      </text>
    </svg>
  );
}

/**
 * An entry's art from the library: a round avatar for an artist, falling back
 * to a monogram, or a square cover for an album or Song, falling back to the
 * vinyl placeholder.
 */
export function StatsArt({
  kind,
  item,
  size,
  className = "",
}: {
  kind: StatsEntryKind;
  item: StatsItem;
  size: number;
  className?: string;
}) {
  if (kind === "artist") {
    return (
      <CoverImage
        coverArt={item.coverArt}
        size={size}
        alt={item.name}
        className={`rounded-full ${className}`}
        fallback={<Monogram name={item.name} />}
      />
    );
  }
  return <CoverImage coverArt={item.coverArt} size={size} alt={item.name} className={className} />;
}

/** The top three stand out in the accent. */
function Rank({ rank }: { rank: number }) {
  return (
    <span className={`tabular-nums ${rank <= 3 ? "font-semibold text-accent" : "text-faint"}`}>
      {rank}
    </span>
  );
}

/** A small caption over an entry's name, like "Top album". */
function Label({ children }: { children: React.ReactNode }) {
  return (
    <div className="truncate text-[10px] font-semibold tracking-wide text-faint uppercase">
      {children}
    </div>
  );
}

/**
 * One artist, album or Song. `tile` stacks big art over the text, for grids;
 * `row` puts a small cover beside it, for lists; `hero` sets large art beside
 * a large name, for the one entry that leads the page.
 */
export function StatsEntry({
  kind,
  item,
  layout,
  label,
  rank,
  count,
  detail,
  album,
}: {
  kind: StatsEntryKind;
  item: StatsItem;
  layout: "tile" | "row" | "hero";
  /** A caption over the name, for `tile` and `hero`. */
  label?: string;
  /** Place in its list, from 1. */
  rank?: number;
  /** Plays. */
  count?: number;
  /** Anything else to say about it, after the plays. */
  detail?: React.ReactNode;
  /** A Song row's album, after its artist. */
  album?: string | null;
}) {
  const t = useT();
  const plays = count !== undefined ? t("count.plays", { n: count }) : null;
  const secondary = (
    <>
      {plays}
      {plays && detail ? " · " : null}
      {detail}
    </>
  );

  if (layout === "hero") {
    return (
      <StatsEntryActions kind={kind} item={item} className={HOVER}>
        <div className="flex min-w-0 items-center gap-5 rounded-lg p-2 sm:gap-6">
          <StatsArt
            kind={kind}
            item={item}
            size={HERO_ART}
            className={`size-28 shrink-0 shadow-xl sm:size-40 ${kind === "artist" ? "" : "rounded-lg"}`}
          />
          <div className="min-w-0">
            {label && <Label>{label}</Label>}
            {/* Two lines before it truncates: the name is the point here. */}
            <div
              className="mt-1 line-clamp-2 text-2xl font-bold tracking-tight break-words sm:text-4xl"
              title={item.name}
            >
              {item.name}
            </div>
            {item.artist && <div className="mt-1 truncate text-sm text-dim">{item.artist}</div>}
            <div className="mt-1.5 text-sm text-dim">{secondary}</div>
          </div>
        </div>
      </StatsEntryActions>
    );
  }

  if (layout === "tile") {
    // Avatars are centred under their circle, covers read from the left.
    const artist = kind === "artist";
    return (
      <StatsEntryActions kind={kind} item={item} className={HOVER}>
        <div className={`min-w-0 rounded-lg p-2 ${artist ? "text-center" : ""}`}>
          <StatsArt
            kind={kind}
            item={item}
            size={TILE_ART}
            className={artist ? "" : "rounded-md"}
          />
          {label && (
            <div className="mt-2">
              <Label>{label}</Label>
            </div>
          )}
          <div className={`${label ? "" : "mt-2"} truncate text-sm font-medium`} title={item.name}>
            {rank !== undefined && (
              <>
                <Rank rank={rank} />{" "}
              </>
            )}
            {item.name}
          </div>
          {/* A non-breaking space holds the line for an album by no one artist
              in particular, so tiles in a row stay the same height. */}
          {!artist && <div className="truncate text-xs text-dim">{item.artist || " "}</div>}
          <div className="truncate text-xs text-faint tabular-nums">{secondary}</div>
        </div>
      </StatsEntryActions>
    );
  }

  return (
    <StatsEntryActions kind={kind} item={item} className={HOVER}>
      <div className="flex min-w-0 items-center gap-3 rounded px-2 py-1.5">
        {rank !== undefined && (
          <span className="w-5 shrink-0 text-right text-xs">
            <Rank rank={rank} />
          </span>
        )}
        <StatsArt
          kind={kind}
          item={item}
          size={ROW_ART}
          className={`size-10 shrink-0 ${kind === "artist" ? "" : "rounded"}`}
        />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm" title={item.name}>
            {item.name}
          </div>
          {(item.artist || album) && (
            <div className="truncate text-xs text-dim">
              {item.artist}
              {album && (
                <span className="text-faint">
                  {item.artist ? " · " : null}
                  {album}
                </span>
              )}
            </div>
          )}
        </div>
        <span className="shrink-0 text-xs tabular-nums text-faint">{secondary}</span>
      </div>
    </StatsEntryActions>
  );
}
