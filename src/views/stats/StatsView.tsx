import { useQuery } from "@tanstack/react-query";
import { BarChart3, RefreshCw } from "lucide-react";
import { Link, useSearchParams } from "react-router";
import { markStatsImportStarted } from "../../lib/playback/engineBridge";
import { useUiStore } from "../../stores/uiStore";
import {
  AreaChart,
  BarRows,
  CalendarHeatmap,
  ClockChart,
  SessionIntervals,
  StatTile,
} from "../../components/stats/charts";
import { StatsEntry, statsEntryKey, type StatsEntryKind } from "../../components/stats/StatsEntry";
import { Button } from "../../components/ui/Button";
import { Input } from "../../components/ui/Input";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { cmd } from "../../lib/ipc/commands";
import { formatRelativeDate } from "../../lib/format";
import { useT, type MessageKey, type T } from "../../lib/i18n";
import type { FirstHeard, StatsRange, TopEntry } from "../../lib/subsonic/types";

const RANGES: { value: StatsRange; labelKey: MessageKey }[] = [
  { value: "day", labelKey: "range.day" },
  { value: "week", labelKey: "range.week" },
  { value: "month", labelKey: "range.month" },
  { value: "year", labelKey: "range.year" },
  { value: "all", labelKey: "range.all" },
  { value: "custom", labelKey: "range.custom" },
];

const WEEKDAY_KEYS: MessageKey[] = [
  "weekday.mon",
  "weekday.tue",
  "weekday.wed",
  "weekday.thu",
  "weekday.fri",
  "weekday.sat",
  "weekday.sun",
];

/** Artists whose first-ever play landed in this period, most-played first. */
function FirstHeardList({ entries }: { entries: FirstHeard[] }) {
  const t = useT();
  if (entries.length === 0) {
    return <p className="px-1 py-4 text-xs text-faint">{t("stats.nothingNew")}</p>;
  }
  return (
    <ol className="flex flex-col">
      {entries.map((entry) => (
        <li
          key={`${entry.name}-${entry.ts}`}
          className="flex items-baseline justify-between gap-3 rounded px-2 py-1.5 odd:bg-surface-2/40"
        >
          <span className="min-w-0 truncate text-sm">{entry.name}</span>
          <span className="shrink-0 text-xs tabular-nums text-faint">
            {t.d(entry.ts * 1000, { day: "numeric", month: "short" })}
            <span className="text-dim">
              {" · "}
              {t("count.plays", { n: entry.plays })}
            </span>
          </span>
        </li>
      ))}
    </ol>
  );
}

/**
 * A top list: art tiles in a grid for artists and albums, two, five or ten
 * to a row so ten entries never leave one dangling; rows with covers for
 * Songs, ranked down each column.
 */
function TopEntries({ kind, entries }: { kind: StatsEntryKind; entries: TopEntry[] }) {
  const t = useT();
  if (entries.length === 0) {
    return <p className="px-1 py-4 text-xs text-faint">{t("stats.noPlays")}</p>;
  }
  const layout = kind === "song" ? "row" : "tile";
  return (
    <ol
      className={
        layout === "tile"
          ? "grid grid-cols-2 gap-1 sm:grid-cols-5 2xl:grid-cols-10"
          : "grid gap-x-6 lg:grid-flow-col lg:grid-cols-2 lg:grid-rows-5"
      }
    >
      {entries.map((entry, i) => (
        <li key={statsEntryKey(entry)} className="min-w-0">
          <StatsEntry
            kind={kind}
            item={entry}
            layout={layout}
            rank={i + 1}
            count={entry.count}
          />
        </li>
      ))}
    </ol>
  );
}

/** `YYYY-MM-DD` for a local date, as `<input type="date">` expects. */
function toDateInput(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/**
 * Local-midnight unix seconds for a date input value, `dayOffset` days later —
 * offset 1 turns the picked end date into the exclusive upper bound.
 */
function dateInputToTs(value: string, dayOffset = 0): number | null {
  const [y, m, d] = value.split("-").map(Number);
  if (!y || !m || !d) return null;
  return Math.floor(new Date(y, m - 1, d + dayOffset).getTime() / 1000);
}

/** The value back, or null when it is not a date input value we can use. */
function validDate(value: string | null): string | null {
  return value && dateInputToTs(value) !== null ? value : null;
}

/** "Aug 2026" for a `YYYY-MM-DD` label. */
function monthLabel(date: string, t: T): string {
  return t.d(`${date}T00:00:00`, { month: "short", year: "numeric" });
}

function daysAgo(days: number): Date {
  const date = new Date();
  date.setDate(date.getDate() - days);
  return date;
}

function Section({
  title,
  hint,
  className = "",
  children,
}: {
  title: string;
  /** Secondary numbers for the header — the section's own context. */
  hint?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <section className={`rounded-xl border border-edge bg-surface p-4 ${className}`}>
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h2 className="text-sm font-semibold text-dim">{title}</h2>
        {hint && <span className="truncate text-[11px] tabular-nums text-faint">{hint}</span>}
      </div>
      {children}
    </section>
  );
}

export function StatsView() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const range = (params.get("range") as StatsRange) ?? "year";
  const today = toDateInput(new Date());
  // Both pickers always hold a usable date: missing or malformed params fall
  // back to the last 30 days rather than leaving the range unresolvable.
  const customFrom = validDate(params.get("from")) ?? toDateInput(daysAgo(30));
  const customTo = validDate(params.get("to")) ?? today;

  // Half-open [from, to) window in unix seconds; presets resolve server-side.
  const from = range === "custom" ? dateInputToTs(customFrom) : null;
  const to = range === "custom" ? dateInputToTs(customTo, 1) : null;

  function selectRange(value: StatsRange) {
    if (value !== "custom") return setParams({ range: value }, { replace: true });
    setParams({ range: "custom", from: customFrom, to: customTo }, { replace: true });
  }

  /** Move one bound, pushing the other along if it would cross over. */
  function setBound(which: "from" | "to", value: string) {
    const next = { from: customFrom, to: customTo, [which]: value };
    if (next.from && next.to && next.from > next.to) {
      if (which === "from") next.to = value;
      else next.from = value;
    }
    setParams({ range: "custom", from: next.from, to: next.to }, { replace: true });
  }

  // Import completion invalidates these globally (engine bridge).
  const sources = useQuery({
    queryKey: ["stats-sources"],
    queryFn: () => cmd("stats_get_sources"),
  });
  const stats = useQuery({
    queryKey: ["stats", range, from, to],
    queryFn: () => cmd("stats_get", { range, from, to }),
    staleTime: 60_000,
  });

  const connected = Boolean(sources.data?.lastfm);
  const summary = stats.data;
  const importProgress = useUiStore((s) => s.statsImport);
  const importing = importProgress !== null || Boolean(sources.data?.importing);

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-3 px-6 py-4">
        <h1 className="mr-2 text-2xl font-semibold">{t("nav.stats")}</h1>
        <div className="flex gap-1">
          {RANGES.map((r) => (
            <button
              key={r.value}
              onClick={() => selectRange(r.value)}
              className={`cursor-pointer rounded-full px-4 py-1.5 text-sm transition-colors ${
                range === r.value
                  ? "bg-surface-2 font-medium text-text"
                  : "text-dim hover:text-text"
              }`}
            >
              {t(r.labelKey)}
            </button>
          ))}
        </div>
        {range === "custom" && (
          <div className="flex items-center gap-2">
            <Input
              type="date"
              aria-label={t("stats.startDate")}
              value={customFrom}
              max={today}
              onChange={(e) => setBound("from", e.target.value)}
              className="w-auto rounded-full px-3 py-1 tabular-nums"
            />
            <span className="text-xs text-faint">{t("common.to")}</span>
            <Input
              type="date"
              aria-label={t("stats.endDate")}
              value={customTo}
              min={customFrom || undefined}
              max={today}
              onChange={(e) => setBound("to", e.target.value)}
              className="w-auto rounded-full px-3 py-1 tabular-nums"
            />
          </div>
        )}
        {connected && (
          <div className="ml-auto flex items-center gap-3">
            {importing && (
              <span className="text-xs tabular-nums text-dim">
                {importProgress && importProgress.imported > 0
                  ? importProgress.total
                    ? t("stats.scrobblesProgressOfTotal", {
                        n: importProgress.imported,
                        total: importProgress.total,
                      })
                    : t("stats.scrobblesProgress", { n: importProgress.imported })
                  : t("stats.contactingLastfm")}
              </span>
            )}
            <Button
              variant="outline"
              className="rounded-full"
              disabled={importing}
              onClick={() => {
                markStatsImportStarted();
                void cmd("stats_import", { source: "lastfm" });
              }}
            >
              <RefreshCw className={`size-4 ${importing ? "animate-spin" : ""}`} />
              {importing ? t("settings.importing") : t("stats.refresh")}
            </Button>
          </div>
        )}
      </div>
      {/* Indeterminate activity bar while an import runs */}
      {importing && (
        <div className="relative -mt-px h-0.5 overflow-hidden">
          <div className="animate-import-sweep absolute inset-y-0 w-1/3 rounded-full bg-accent" />
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {!connected && !sources.isPending ? (
          <div className="flex flex-col items-center gap-3 py-24 text-dim">
            <BarChart3 className="size-10 text-faint" />
            <p className="max-w-sm text-center text-sm">
              {t("stats.noSourcesLead")}{" "}
              <Link to="/settings" className="text-accent hover:underline">
                {t("stats.noSourcesLink")}
              </Link>{" "}
              {t("stats.noSourcesTail")}
            </p>
          </div>
        ) : stats.isPending ? (
          <CenteredSpinner />
        ) : summary && summary.totalPlays === 0 ? (
          <div className="py-24 text-center text-sm text-dim">
            {connected ? t("stats.noPlaysRefresh") : t("stats.noPlays")}
          </div>
        ) : summary ? (
          <div className="flex flex-col gap-4 px-6 pb-8">
            {/* Headline tiles */}
            <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
              <StatTile
                label={t("stats.plays")}
                value={t.n(summary.totalPlays)}
                trend={summary.timeline.map((point) => point.count)}
              />
              <StatTile label={t("nav.artists")} value={t.n(summary.uniqueArtists)} />
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
            </div>

            <Section title={t("stats.activity")}>
              <AreaChart points={summary.timeline} />
            </Section>

            {summary.daily.length > 0 && (
              <Section
                title={t("stats.calendar")}
                // Long ranges are capped to the most recent year, so the hint
                // states the span it actually covers.
                hint={t("stats.calendarHint", {
                  from: monthLabel(summary.daily[0].label, t),
                  to: monthLabel(summary.daily[summary.daily.length - 1].label, t),
                  active: summary.daily.filter((d) => d.count > 0).length,
                  total: summary.daily.length,
                })}
              >
                <CalendarHeatmap days={summary.daily} />
              </Section>
            )}

            <div className="grid gap-4 lg:grid-cols-2">
              <Section title={t("stats.aroundTheClock")}>
                <ClockChart counts={summary.byHour} />
              </Section>
              <Section title={t("stats.byWeekday")}>
                <BarRows
                  items={summary.byWeekday.map((count, d) => ({
                    label: t(WEEKDAY_KEYS[d]),
                    count,
                  }))}
                />
              </Section>
            </div>

            {/* Discovery needs a window to be meaningful, so the all-time
                range gives the sessions the full width instead. */}
            <div className={`grid gap-4 ${summary.discovery ? "lg:grid-cols-2" : ""}`}>
              <Section
                title={t("stats.longestSessions")}
                hint={
                  summary.sessions.count > 0
                    ? t("stats.sessionsHint", {
                        sessions: t("count.sessions", { n: summary.sessions.count }),
                        median: summary.sessions.medianTracks,
                      })
                    : undefined
                }
              >
                <SessionIntervals sessions={summary.sessions.longest} />
              </Section>
              {summary.discovery && (
                <Section
                  title={t("stats.newToYou")}
                  hint={t("stats.discoveryHint", {
                    artists: t("count.artists", { n: summary.discovery.newArtists }),
                    songs: t("count.songs", { n: summary.discovery.newTracks }),
                  })}
                >
                  <FirstHeardList entries={summary.discovery.firstArtists} />
                </Section>
              )}
            </div>

            <Section title={t("stats.topArtists")}>
              <TopEntries kind="artist" entries={summary.topArtists} />
            </Section>
            <Section title={t("stats.topAlbums")}>
              <TopEntries kind="album" entries={summary.topAlbums} />
            </Section>
            <Section title={t("stats.topSongs")}>
              <TopEntries kind="song" entries={summary.topSongs} />
            </Section>

            {/* Flex, not a 3-column grid: fewer years should widen to fill the
                row rather than leave a hole. */}
            {summary.onThisDay.length > 0 && (
              <div className="flex flex-wrap gap-4">
                {summary.onThisDay.map((day) => (
                  <Section
                    key={day.year}
                    className="min-w-64 flex-1"
                    title={t("stats.onThisDay", { year: String(day.year) })}
                    hint={t("count.plays", { n: day.plays })}
                  >
                    <ol className="flex flex-col gap-1.5">
                      {day.tracks.map((track) => (
                        <li key={`${track.name}-${track.artist ?? ""}`} className="min-w-0">
                          <div className="truncate text-sm">{track.name}</div>
                          <div className="truncate text-xs text-dim">
                            {track.artist}
                            {track.count > 1 && (
                              <span className="text-faint"> · {t.n(track.count)}×</span>
                            )}
                          </div>
                        </li>
                      ))}
                    </ol>
                  </Section>
                ))}
              </div>
            )}

            <Section title={t("stats.recentScrobbles")}>
              {summary.recent.length === 0 ? (
                <p className="px-1 py-4 text-xs text-faint">{t("stats.noPlays")}</p>
              ) : (
                <ol className="flex flex-col">
                  {summary.recent.map((play, i) => (
                    <li
                      key={`${play.ts}-${i}`}
                      className="flex items-baseline justify-between gap-4 rounded px-2 py-1.5 odd:bg-surface-2/40"
                    >
                      <span className="min-w-0 truncate text-sm">
                        {play.track}
                        <span className="text-dim"> — {play.artist}</span>
                        {play.album && <span className="text-faint"> · {play.album}</span>}
                      </span>
                      <span className="shrink-0 text-xs tabular-nums text-faint">
                        {formatRelativeDate(new Date(play.ts * 1000).toISOString(), t)}
                      </span>
                    </li>
                  ))}
                </ol>
              )}
            </Section>
          </div>
        ) : null}
      </div>
    </div>
  );
}
