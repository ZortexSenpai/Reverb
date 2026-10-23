// The Period's headline numbers, in one strip below the hero. The strip sizes
// its own columns from however many tiles it holds, so a new number is just
// one more StatTile.

import { Children, useLayoutEffect, useRef, useState } from "react";
import { balancedColumns } from "../../lib/balancedColumns";
import { useT } from "../../lib/i18n";
import type { StatsSummary } from "../../lib/subsonic/types";
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

export function HeadlineStrip({ summary }: { summary: StatsSummary }) {
  const t = useT();
  return (
    <Strip>
      <StatTile
        label={t("stats.plays")}
        value={t.n(summary.totalPlays)}
        trend={summary.timeline.map((point) => point.count)}
      />
      <StatTile label={t("nav.artists")} value={t.n(summary.uniqueArtists)} />
      <StatTile label={t("nav.albums")} value={t.n(summary.uniqueAlbums)} />
      <StatTile label={t("nav.songs")} value={t.n(summary.uniqueSongs)} />
      <StatTile
        label={t("stats.playsPerDay")}
        value={t.n(summary.playsPerDay, {
          minimumFractionDigits: 1,
          maximumFractionDigits: 1,
        })}
        hint={
          summary.firstTs
            ? t("stats.since", {
                date: t.d(summary.firstTs * 1000, { dateStyle: "medium" }),
              })
            : undefined
        }
      />
      <StatTile
        label={t("stats.longestStreak")}
        value={t("stats.streakDays", { n: summary.longestStreakDays })}
        hint={t("stats.streakHint")}
      />
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
