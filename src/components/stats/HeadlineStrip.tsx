// The Period's headline numbers, in one strip below the hero. The strip sizes
// its own columns from however many tiles it holds, so a new number is just
// one more StatTile. Each count shows its change since the Previous period.

import { Children, useLayoutEffect, useRef, useState } from "react";
import { balancedColumns } from "../../lib/balancedColumns";
import { formatChange, formatCoverage, formatListeningTime } from "../../lib/format";
import { useT } from "../../lib/i18n";
import type { StatsSummary } from "../../lib/subsonic/types";
import { Spinner } from "../ui/Spinner";
import { StatTile } from "./charts";

/** Narrowest a tile gets before the strip wraps. */
const TILE_MIN_WIDTH = 150;
const GAP = 12;

/** Tiles in as few rows as fit, each row filled about evenly. */
function Strip({ children }: { children: React.ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    // Measured before the first paint, so the tiles never jump.
    setWidth(el.clientWidth);
    const ro = new ResizeObserver((entries) => setWidth(entries[0].contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const fit = Math.floor((width + GAP) / (TILE_MIN_WIDTH + GAP));
  // toArray drops the `false` a conditional tile leaves behind.
  const columns = balancedColumns(Children.toArray(children).length, fit);
  return (
    <div
      ref={ref}
      className="grid gap-3"
      style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
    >
      {children}
    </div>
  );
}

/** A count's change since the Previous period, like "+12%". */
function Change({ value }: { value: number }) {
  const t = useT();
  // Whatever rounds to "0%" reads as no change at all.
  const percent = Math.round(value * 100);
  const tone =
    percent > 0 ? "bg-accent/15 text-accent" : percent < 0 ? "bg-surface-2 text-dim" : "text-faint";
  return (
    <span title={t("stats.vsPrevious")} className={`rounded-full px-1.5 py-px font-medium ${tone}`}>
      {formatChange(value, t)}
    </span>
  );
}

/** The change badge for a tile, when there's a Previous period to compare with. */
function changeBadge(value: number | null | undefined) {
  return value == null ? undefined : <Change value={value} />;
}

/**
 * An estimate from the library Songs' lengths, so it says how much of the
 * Period's plays it comes from.
 */
function ListeningTimeTile({
  time,
  change,
}: {
  time: StatsSummary["listeningTime"];
  change?: React.ReactNode;
}) {
  const t = useT();
  const label = t("stats.listeningTime");
  // Absent until Reverb has read the library.
  if (!time) {
    return (
      <StatTile
        label={label}
        value={<Spinner className="my-1.5 size-5" />}
        hint={t("stats.readingLibrary")}
      />
    );
  }
  // Not one play with a known length: nothing to estimate from.
  if (time.secs === 0) return <StatTile label={label} value="—" />;
  return (
    <StatTile
      label={label}
      value={formatListeningTime(time.secs, t)}
      hint={formatCoverage(time.coverage, t)}
      change={change}
    />
  );
}

export function HeadlineStrip({ summary }: { summary: StatsSummary }) {
  const t = useT();
  // Absent for all time, which has no Previous period.
  const changes = summary.changes;
  return (
    <Strip>
      <StatTile
        label={t("stats.plays")}
        value={t.n(summary.totalPlays)}
        change={changeBadge(changes?.plays)}
        trend={summary.timeline.map((point) => point.count)}
      />
      <ListeningTimeTile
        time={summary.listeningTime}
        change={changeBadge(changes?.listeningTime)}
      />
      <StatTile
        label={t("nav.artists")}
        value={t.n(summary.uniqueArtists)}
        change={changeBadge(changes?.artists)}
      />
      <StatTile
        label={t("nav.albums")}
        value={t.n(summary.uniqueAlbums)}
        change={changeBadge(changes?.albums)}
      />
      <StatTile
        label={t("nav.songs")}
        value={t.n(summary.uniqueSongs)}
        change={changeBadge(changes?.songs)}
      />
      <StatTile
        label={t("stats.playsPerDay")}
        value={t.n(summary.playsPerDay, {
          minimumFractionDigits: 1,
          maximumFractionDigits: 1,
        })}
        change={changeBadge(changes?.playsPerDay)}
        hint={
          summary.firstTs
            ? t("stats.since", {
                date: t.d(summary.firstTs * 1000, { dateStyle: "medium" }),
              })
            : undefined
        }
      />
      {/* Not a count of the Period, so it isn't compared. */}
      <StatTile
        label={t("stats.currentStreak")}
        value={t("stats.streakDays", { n: summary.currentStreakDays })}
        hint={t("stats.currentStreakHint")}
      />
      <StatTile
        label={t("stats.longestStreak")}
        value={t("stats.streakDays", { n: summary.longestStreakDays })}
        hint={t("stats.streakHint")}
        change={changeBadge(changes?.longestStreak)}
      />
      {/* One day, not a count of the Period: not compared either. */}
      <StatTile
        label={t("stats.busiestDay")}
        value={summary.busiestDay ? t.n(summary.busiestDay.count) : "—"}
        hint={
          summary.busiestDay
            ? t("stats.playsOn", {
                // `label` is a plain "2026-08-28"; anchor it at local
                // midnight so it is not shifted a day by the timezone.
                date: t.d(`${summary.busiestDay.label}T00:00:00`, {
                  dateStyle: "medium",
                }),
              })
            : undefined
        }
      />
    </Strip>
  );
}
