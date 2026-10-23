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
} from "../../components/stats/charts";
import { ArtistsOverTime } from "../../components/stats/ArtistsOverTime";
import { HeadlineStrip } from "../../components/stats/HeadlineStrip";
import { StatsEntry, statsEntryKey, type StatsEntryKind } from "../../components/stats/StatsEntry";
import { StatsHero } from "../../components/stats/StatsHero";
import { Button } from "../../components/ui/Button";
import { Input } from "../../components/ui/Input";
import { CenteredSpinner, Spinner } from "../../components/ui/Spinner";
import { cmd } from "../../lib/ipc/commands";
import { formatRelativeDate } from "../../lib/format";
import { useT, type MessageKey, type T } from "../../lib/i18n";
import type {
  Breakdown,
  FirstHeard,
  GenrePlays,
  StatsRange,
  StatsSummary,
  TopEntry,
} from "../../lib/subsonic/types";

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
        <li key={statsEntryKey(entry)} className="min-w-0">
          <StatsEntry
            kind="artist"
            item={entry}
            layout="row"
            count={entry.plays}
            detail={t.d(entry.ts * 1000, { day: "numeric", month: "short" })}
          />
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

/** Stand-in for a section built from the library while Reverb reads it. */
function ReadingLibrary() {
  const t = useT();
  return (
    <div className="flex items-center gap-2 px-1 py-4 text-xs text-faint">
      <Spinner className="size-3.5" />
      {t("stats.readingLibrary")}
    </div>
  );
}

/**
 * Genres ranked by plays. No shares: a Song counts toward each of its
 * genres, so they would add up to more than 100%.
 */
function GenreList({ genres }: { genres: GenrePlays[] }) {
  const t = useT();
  const max = Math.max(1, ...genres.map((g) => g.count));
  return (
    <ol className="flex flex-col gap-1.5">
      {genres.map((genre, i) => (
        <li key={genre.name}>
          <div className="flex items-baseline justify-between gap-3">
            <span className="min-w-0 truncate text-sm">
              <span
                className={`mr-1.5 text-xs tabular-nums ${
                  i < 3 ? "font-semibold text-accent" : "text-faint"
                }`}
              >
                {i + 1}.
              </span>
              {genre.name}
            </span>
            <span className="shrink-0 text-xs tabular-nums text-dim">
              {t("count.plays", { n: genre.count })}
            </span>
          </div>
          <div className="mt-1 h-1 overflow-hidden rounded-full bg-surface-2">
            <div
              className="h-full rounded-full bg-accent"
              style={{ width: `${(genre.count / max) * 100}%` }}
            />
          </div>
        </li>
      ))}
    </ol>
  );
}

/** "from 82% of plays", for a breakdown with something in it. */
function coverageHint(breakdown: Breakdown<unknown> | null | undefined, t: T) {
  if (!breakdown?.entries.length) return undefined;
  return t("stats.coverage", {
    share: t.n(breakdown.coverage, { style: "percent", maximumFractionDigits: 0 }),
  });
}

/** Genres and decades of the Period's library Songs, side by side. */
function GenresAndDecades({ genres, decades }: Pick<StatsSummary, "genres" | "decades">) {
  const t = useT();
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Section title={t("nav.genres")} hint={coverageHint(genres, t)}>
        {!genres ? (
          <ReadingLibrary />
        ) : genres.entries.length === 0 ? (
          <p className="px-1 py-4 text-xs text-faint">{t("stats.noGenres")}</p>
        ) : (
          <GenreList genres={genres.entries} />
        )}
      </Section>
      <Section title={t("stats.decades")} hint={coverageHint(decades, t)}>
        {!decades ? (
          <ReadingLibrary />
        ) : decades.entries.length === 0 ? (
          <p className="px-1 py-4 text-xs text-faint">{t("stats.noDecades")}</p>
        ) : (
          <BarRows
            labelWidth="w-20"
            items={decades.entries.map((d) => ({
              label: t("stats.decade", { decade: String(d.decade) }),
              count: d.count,
            }))}
          />
        )}
      </Section>
    </div>
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
            {/* One scroll, in the spec's order: the Period at a glance, its top
                lists, the listening charts, then the history sections. */}
            {summary.hero && <StatsHero hero={summary.hero} />}
            <HeadlineStrip summary={summary} />

            <Section title={t("stats.topArtists")}>
              <TopEntries kind="artist" entries={summary.topArtists} />
            </Section>
            <Section title={t("stats.topAlbums")}>
              <TopEntries kind="album" entries={summary.topAlbums} />
            </Section>
            <Section title={t("stats.topSongs")}>
              <TopEntries kind="song" entries={summary.topSongs} />
            </Section>

            <Section title={t("stats.activity")}>
              <AreaChart points={summary.timeline} />
            </Section>

            {summary.topArtistsOverTime.length > 0 && (
              <Section title={t("stats.topArtistsOverTime")} hint={t("stats.overTimeHint")}>
                <ArtistsOverTime artists={summary.topArtistsOverTime} />
              </Section>
            )}

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

            {/* Discovery needs a window to be meaningful, so the all-time
                range leaves it out. */}
            {summary.discovery && (
              <Section
                title={t("stats.newToYou")}
                hint={t("stats.discoveryHint", {
                  artists: t("count.artists", { n: summary.discovery.newArtists }),
                  songs: t("count.songs", { n: summary.discovery.newSongs }),
                })}
              >
                <FirstHeardList entries={summary.discovery.firstArtists} />
              </Section>
            )}

            <GenresAndDecades genres={summary.genres} decades={summary.decades} />

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
                    <ol className="flex flex-col">
                      {day.songs.map((song) => (
                        <li key={statsEntryKey(song)} className="min-w-0">
                          <StatsEntry
                            kind="song"
                            item={song}
                            layout="row"
                            detail={song.count > 1 ? `${t.n(song.count)}×` : undefined}
                          />
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
                    <li key={`${play.ts}-${i}`} className="min-w-0">
                      <StatsEntry
                        kind="song"
                        item={play}
                        layout="row"
                        album={play.album}
                        detail={formatRelativeDate(new Date(play.ts * 1000).toISOString(), t)}
                      />
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
