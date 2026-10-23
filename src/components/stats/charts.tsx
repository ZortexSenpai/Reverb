// Chart primitives for the stats page. Each dataset gets the form that fits
// its job: area chart for change-over-time, radial clock for the cyclical
// hours, horizontal bars for the 7 weekdays and the decades. Everything plots
// ONE measure (plays): a single accent hue carries magnitude, text wears text
// tokens, grids stay recessive, and every mark has a hover target with a
// tooltip.
// Top lists are entries with art, not charts (StatsEntry).

import { useId, useMemo, useRef, useState } from "react";
import { localDate } from "../../lib/format";
import { useT, type MessageKey, type T } from "../../lib/i18n";
import type { ListeningSession, TimelinePoint } from "../../lib/subsonic/types";

// ---------------------------------------------------------------------------
// Area chart (timeline)
// ---------------------------------------------------------------------------

const AREA_W = 100;
const AREA_H = 40;

export function AreaChart({
  points,
  maxLabels = 6,
  max: sharedMax,
  className = "h-36",
}: {
  points: TimelinePoint[];
  /** Bucket labels along the bottom, at most; 0 for none. */
  maxLabels?: number;
  /** Top of the scale, for charts set side by side; else the highest point. */
  max?: number;
  /** The plot's height. */
  className?: string;
}) {
  const t = useT();
  const [hover, setHover] = useState<number | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  // Several charts can share a page, so each needs its own gradient id. Only
  // plain characters, so the `url(#…)` reference resolves in every engine.
  const fillId = `area-fill-${useId().replace(/[^\w-]/g, "")}`;
  const max = Math.max(1, sharedMax ?? 0, ...points.map((p) => p.count));
  const n = points.length;

  const { linePath, areaPath } = useMemo(() => {
    if (n === 0) return { linePath: "", areaPath: "" };
    const x = (i: number) => (n === 1 ? AREA_W / 2 : (i / (n - 1)) * AREA_W);
    const y = (count: number) => AREA_H - (count / max) * (AREA_H - 3);
    const line = points
      .map((p, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(2)},${y(p.count).toFixed(2)}`)
      .join("");
    return {
      linePath: line,
      areaPath: `${line}L${AREA_W},${AREA_H}L0,${AREA_H}Z`,
    };
  }, [points, max, n]);

  if (n === 0) return null;
  const labelEvery = maxLabels > 0 ? Math.max(1, Math.ceil(n / maxLabels)) : 0;

  function onMove(e: React.MouseEvent) {
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const t = (e.clientX - rect.left) / rect.width;
    setHover(Math.min(n - 1, Math.max(0, Math.round(t * (n - 1)))));
  }

  const hoverX = hover !== null ? (n === 1 ? 50 : (hover / (n - 1)) * 100) : 0;
  const hoverY =
    hover !== null ? ((AREA_H - (points[hover].count / max) * (AREA_H - 3)) / AREA_H) * 100 : 0;

  return (
    <div className={labelEvery ? "pb-4" : ""}>
      <div
        ref={containerRef}
        className={`relative ${className}`}
        onMouseMove={onMove}
        onMouseLeave={() => setHover(null)}
      >
        <svg
          viewBox={`0 0 ${AREA_W} ${AREA_H}`}
          preserveAspectRatio="none"
          className="absolute inset-0 size-full"
        >
          <defs>
            <linearGradient id={fillId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--color-accent)" stopOpacity="0.35" />
              <stop offset="100%" stopColor="var(--color-accent)" stopOpacity="0.02" />
            </linearGradient>
          </defs>
          {[0.5, 1].map((t) => (
            <line
              key={t}
              x1={0}
              x2={AREA_W}
              y1={AREA_H - t * (AREA_H - 3)}
              y2={AREA_H - t * (AREA_H - 3)}
              className="stroke-edge"
              strokeWidth={1}
              vectorEffect="non-scaling-stroke"
            />
          ))}
          <path d={areaPath} fill={`url(#${fillId})`} />
          <path
            d={linePath}
            fill="none"
            stroke="var(--color-accent)"
            strokeWidth={2}
            vectorEffect="non-scaling-stroke"
            strokeLinejoin="round"
          />
        </svg>
        {/* HTML crosshair + marker (no SVG distortion) */}
        {hover !== null && (
          <>
            <div
              className="pointer-events-none absolute top-0 bottom-0 w-px bg-dim/40"
              style={{ left: `${hoverX}%` }}
            />
            <div
              className="pointer-events-none absolute size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-bg bg-accent-bright"
              style={{ left: `${hoverX}%`, top: `${hoverY}%` }}
            />
            <div
              className="pointer-events-none absolute -top-2 z-10 -translate-x-1/2 rounded-md border border-edge bg-surface-2 px-2 py-1 text-xs whitespace-nowrap shadow-lg"
              style={{ left: `${hoverX}%` }}
            >
              {t("chart.playsWithLabel", {
                label: points[hover].label,
                n: points[hover].count,
              })}
            </div>
          </>
        )}
      </div>
      {labelEvery > 0 && (
        <div className="flex justify-between pt-1 text-[10px] text-faint">
          {points
            .filter((_, i) => i % labelEvery === 0)
            .map((p, i) => (
              <span key={p.label + i}>{p.label}</span>
            ))}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Radial clock chart (hour-of-day)
// ---------------------------------------------------------------------------

const CLOCK_SIZE = 240;
const CLOCK_R0 = 40;
const CLOCK_R1 = 104;

function polar(cx: number, cy: number, r: number, deg: number): [number, number] {
  const rad = ((deg - 90) * Math.PI) / 180;
  return [cx + r * Math.cos(rad), cy + r * Math.sin(rad)];
}

function wedge(cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): string {
  const [x0o, y0o] = polar(cx, cy, r1, a0);
  const [x1o, y1o] = polar(cx, cy, r1, a1);
  const [x0i, y0i] = polar(cx, cy, r0, a0);
  const [x1i, y1i] = polar(cx, cy, r0, a1);
  return [
    `M${x0i.toFixed(2)},${y0i.toFixed(2)}`,
    `L${x0o.toFixed(2)},${y0o.toFixed(2)}`,
    `A${r1},${r1} 0 0 1 ${x1o.toFixed(2)},${y1o.toFixed(2)}`,
    `L${x1i.toFixed(2)},${y1i.toFixed(2)}`,
    `A${r0},${r0} 0 0 0 ${x0i.toFixed(2)},${y0i.toFixed(2)}`,
    "Z",
  ].join("");
}

export function ClockChart({ counts }: { counts: number[] }) {
  const t = useT();
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(1, ...counts);
  const c = CLOCK_SIZE / 2;

  return (
    <div className="relative mx-auto w-fit">
      <svg viewBox={`0 0 ${CLOCK_SIZE} ${CLOCK_SIZE}`} className="h-60 w-60">
        {/* Recessive reference rings */}
        {[CLOCK_R0, (CLOCK_R0 + CLOCK_R1) / 2, CLOCK_R1].map((r) => (
          <circle key={r} cx={c} cy={c} r={r} fill="none" className="stroke-edge" strokeWidth={0.75} />
        ))}
        {counts.map((count, hour) => {
          const a0 = hour * 15 + 1.5;
          const a1 = (hour + 1) * 15 - 1.5;
          const r1 = CLOCK_R0 + (count / max) * (CLOCK_R1 - CLOCK_R0);
          return (
            <g key={hour}>
              {count > 0 && (
                <path
                  d={wedge(c, c, CLOCK_R0 + 1, Math.max(r1, CLOCK_R0 + 3), a0, a1)}
                  className={hover === hour ? "fill-accent-bright" : "fill-accent"}
                  opacity={hover === null || hover === hour ? 1 : 0.5}
                />
              )}
              {/* Full-ring hover target */}
              <path
                d={wedge(c, c, CLOCK_R0, CLOCK_R1 + 8, hour * 15, (hour + 1) * 15)}
                fill="transparent"
                onMouseEnter={() => setHover(hour)}
                onMouseLeave={() => setHover(null)}
              />
            </g>
          );
        })}
        {/* Clock labels */}
        {[0, 6, 12, 18].map((hour) => {
          const [x, y] = polar(c, c, CLOCK_R1 + 14, hour * 15 + 7.5);
          return (
            <text
              key={hour}
              x={x}
              y={y + 3}
              textAnchor="middle"
              className="fill-faint text-[10px]"
            >
              {hour}
            </text>
          );
        })}
        <text x={c} y={c - 2} textAnchor="middle" className="fill-text text-sm font-semibold">
          {t.n(hover !== null ? counts[hover] : counts.reduce((a, b) => a + b, 0))}
        </text>
        <text x={c} y={c + 12} textAnchor="middle" className="fill-faint text-[9px]">
          {hover !== null
            ? t("chart.hourRange", { from: hover, to: (hover + 1) % 24 })
            : t("chart.plays")}
        </text>
      </svg>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Horizontal bars (weekdays, decades)
// ---------------------------------------------------------------------------

export function BarRows({
  items,
  labelWidth = "w-9",
}: {
  items: { label: string; count: number }[];
  /** Width class for the label column: wide enough for the longest label. */
  labelWidth?: string;
}) {
  const t = useT();
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(1, ...items.map((i) => i.count));
  return (
    <div className="flex flex-col gap-2 py-1">
      {items.map((item, i) => (
        <div
          key={item.label}
          className="flex items-center gap-3"
          onMouseEnter={() => setHover(i)}
          onMouseLeave={() => setHover(null)}
        >
          <span className={`${labelWidth} shrink-0 text-xs text-dim`}>{item.label}</span>
          <div className="h-3.5 flex-1 overflow-hidden rounded-r">
            <div
              className={`h-full rounded-r transition-colors ${
                hover === i ? "bg-accent-bright" : "bg-accent"
              }`}
              style={{ width: `${(item.count / max) * 100}%` }}
            />
          </div>
          <span className="w-14 shrink-0 text-right text-xs tabular-nums text-dim">
            {t.n(item.count)}
          </span>
        </div>
      ))}
    </div>
  );
}

export function StatTile({
  label,
  value,
  hint,
  change,
  trend,
}: {
  label: string;
  /** The number, or a spinner while it's still coming. */
  value: React.ReactNode;
  /** A line of context under the label. */
  hint?: string;
  /** A small badge beside the value, like its change since the Previous period. */
  change?: React.ReactNode;
  /** Optional shape-of-the-period sparkline, oldest value first. */
  trend?: number[];
}) {
  return (
    <div className="flex min-w-0 flex-col rounded-xl border border-edge bg-surface p-4">
      {/* The badge drops under the value when a narrow tile can't fit both. */}
      <div className="flex flex-wrap items-baseline justify-between gap-x-2">
        {/* Proportional figures: tabular-nums looks loose at display sizes. */}
        <div className="truncate text-2xl font-semibold">{value}</div>
        {change && <div className="shrink-0 text-[11px] tabular-nums">{change}</div>}
      </div>
      <div className="mt-0.5 truncate text-xs text-dim">{label}</div>
      {hint && <div className="text-[10px] text-faint">{hint}</div>}
      {/* Pinned to the bottom, level with the strip's other tiles. */}
      {trend && trend.length > 2 && (
        <div className="mt-auto">
          <Sparkline values={trend} />
        </div>
      )}
    </div>
  );
}

const SPARK_W = 100;
const SPARK_H = 16;
const SPARK_POINTS = 16;

/** Recessive shape-only line: the tile's number carries the value. */
function Sparkline({ values }: { values: number[] }) {
  const points = useMemo(() => {
    // Even sampling keeps the line readable however long the range is.
    const step = Math.max(1, Math.ceil(values.length / SPARK_POINTS));
    return values.filter((_, i) => i % step === 0 || i === values.length - 1);
  }, [values]);

  const max = Math.max(1, ...points);
  const x = (i: number) => (points.length === 1 ? SPARK_W : (i / (points.length - 1)) * SPARK_W);
  const y = (v: number) => SPARK_H - (v / max) * (SPARK_H - 2) - 1;
  const path = points.map((v, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)},${y(v).toFixed(1)}`);

  return (
    <svg
      viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
      preserveAspectRatio="none"
      aria-hidden
      className="mt-2 h-4 w-full"
    >
      <path
        d={path.join("")}
        fill="none"
        className="stroke-faint"
        strokeWidth={1.5}
        vectorEffect="non-scaling-stroke"
        strokeLinejoin="round"
      />
      {/* Where the period ends — the only point worth marking. */}
      <circle
        cx={x(points.length - 1)}
        cy={y(points[points.length - 1])}
        r={2}
        className="fill-accent"
      />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Calendar heatmap (a cell per day)
// ---------------------------------------------------------------------------

/** Sequential ramp: one hue, light → dark. Level 0 is "no plays", not a step. */
const LEVEL_OPACITY = [0.28, 0.5, 0.74, 1];
/** Only alternate rows are labelled; "" leaves the row blank. */
const WEEKDAY_LABEL_KEYS: (MessageKey | "")[] = [
  "weekday.mon",
  "",
  "weekday.wed",
  "",
  "weekday.fri",
  "",
  "",
];

/** Short month name for a 1-based month number. */
export function monthName(month: number, t: T): string {
  return t(`month.${month}` as MessageKey);
}

/**
 * Plays per day as a week-per-column grid. Its job is consistency at a glance —
 * which the timeline, being aggregated into buckets, cannot show.
 */
export function CalendarHeatmap({ days }: { days: TimelinePoint[] }) {
  const t = useT();
  const [hover, setHover] = useState<number | null>(null);

  const { cells, columns, lead, thresholds, titles } = useMemo(() => {
    // Pad so every column is a full Mon–Sun week.
    const lead = days.length > 0 ? (localDate(days[0].label).getDay() + 6) % 7 : 0;
    const cells: (TimelinePoint | null)[] = [...Array(lead).fill(null), ...days];
    const columns: (TimelinePoint | null)[][] = [];
    for (let i = 0; i < cells.length; i += 7) columns.push(cells.slice(i, i + 7));

    // Tooltips are built here rather than per render: hovering re-renders every
    // cell and there is up to a year of them. The label is a plain
    // "2026-08-28", anchored at local midnight so the date it prints is the day
    // it names, and formatted like every other date on the page rather than
    // shown raw.
    const titles = cells.map((cell) =>
      cell === null
        ? ""
        : t("chart.playsWithLabel", {
            label: t.d(localDate(cell.label), { dateStyle: "medium" }),
            n: cell.count,
          }),
    );

    // Quartiles of the days that had plays, not a linear scale: someone who
    // listens daily would otherwise sit at the top step all year.
    const busy = days
      .map((d) => d.count)
      .filter((c) => c > 0)
      .sort((a, b) => a - b);
    const quantile = (q: number) => busy[Math.min(busy.length - 1, Math.floor(busy.length * q))];
    return {
      cells,
      columns,
      lead,
      titles,
      thresholds: [quantile(0.25), quantile(0.5), quantile(0.75)],
    };
    // `t` is cached per locale, so this only re-runs on a language change.
  }, [days, t]);

  if (days.length === 0) return null;

  const level = (count: number) => {
    if (count === 0) return -1;
    // Degenerate spread (every busy day identical): one mid step reads better
    // than painting them all the lightest.
    if (thresholds[0] >= thresholds[2]) return 2;
    return thresholds.filter((t) => count > t).length;
  };
  const hovered = hover === null ? null : days[hover];

  // Weeks share the width as equal fractions, so the grid fills the panel at
  // any window size instead of sitting in a fixed-width block.
  const template = `1.75rem repeat(${columns.length}, minmax(0, 1fr))`;

  return (
    <div className="relative">
      {/* Month labels, on the same columns as the grid below. */}
      <div className="grid items-end gap-1" style={{ gridTemplateColumns: template }}>
        <span />
        {columns.map((week, col) => {
          const first = week.find((cell) => cell?.label.endsWith("-01"));
          return (
            <span key={col} className="text-[9px] text-faint">
              {first ? monthName(Number(first.label.slice(5, 7)), t) : ""}
            </span>
          );
        })}
      </div>
      <div className="mt-1 grid gap-1" style={{ gridTemplateColumns: template }}>
        {WEEKDAY_LABEL_KEYS.map((labelKey, row) => (
          <span
            key={row}
            className="self-center text-[9px] leading-none text-faint"
            style={{ gridColumn: 1, gridRow: row + 1 }}
          >
            {labelKey === "" ? "" : t(labelKey)}
          </span>
        ))}
        {cells.map((cell, i) =>
          cell === null ? null : (
            <div
              key={cell.label}
              onMouseEnter={() => setHover(i - lead)}
              onMouseLeave={() => setHover(null)}
              title={titles[i]}
              style={{
                gridColumn: Math.floor(i / 7) + 2,
                gridRow: (i % 7) + 1,
                background:
                  level(cell.count) < 0
                    ? "var(--color-surface-2)"
                    : `color-mix(in oklab, var(--color-accent) ${
                        LEVEL_OPACITY[level(cell.count)] * 100
                      }%, transparent)`,
              }}
              className="aspect-square rounded-[2px]"
            />
          ),
        )}
      </div>
      {/* Scale legend — a sequential ramp needs one. */}
      <div className="mt-2 flex items-center gap-1.5 text-[10px] text-faint">
        <span className="tabular-nums">
          {hovered
            ? t("chart.playsWithLabel", {
                label: t.d(localDate(hovered.label), { dateStyle: "medium" }),
                n: hovered.count,
              })
            : t("count.days", { n: days.length })}
        </span>
        <span className="ml-auto">{t("chart.less")}</span>
        <span className="size-2.5 rounded-[2px] bg-surface-2" />
        {LEVEL_OPACITY.map((opacity) => (
          <span
            key={opacity}
            className="size-2.5 rounded-[2px]"
            style={{
              background: `color-mix(in oklab, var(--color-accent) ${opacity * 100}%, transparent)`,
            }}
          />
        ))}
        <span>{t("chart.more")}</span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Session intervals (when a sitting started, how long it ran)
// ---------------------------------------------------------------------------

const DAY_HOURS = 24;
/** Date+artist · the 24-hour track · songs and length. */
const SESSION_COLUMNS = "6.5rem 1fr 9rem";

export function SessionIntervals({ sessions }: { sessions: ListeningSession[] }) {
  const t = useT();
  const [hover, setHover] = useState<number | null>(null);
  if (sessions.length === 0) {
    return <p className="px-1 py-4 text-xs text-faint">{t("chart.noSessions")}</p>;
  }

  return (
    <div className="flex flex-col gap-2 py-1">
      {sessions.map((session, i) => {
        const start = new Date(session.startTs * 1000);
        const startHour = start.getHours() + start.getMinutes() / 60;
        const hours = Math.max(session.spanMinutes / 60, 0.25);
        // Clamp at midnight; the text still states the real length.
        const width = Math.min(hours, DAY_HOURS - startHour);
        return (
          <div
            key={session.startTs}
            className="grid items-center gap-3 text-xs"
            style={{ gridTemplateColumns: SESSION_COLUMNS }}
            onMouseEnter={() => setHover(i)}
            onMouseLeave={() => setHover(null)}
            title={t("chart.sessionTitle", {
              date: t.d(start, { dateStyle: "medium", timeStyle: "short" }),
              n: session.songs,
              span: formatSpan(session.spanMinutes, t),
            })}
          >
            <span className="min-w-0">
              <span className="block truncate text-dim">
                {t.d(start, { day: "numeric", month: "short" })}
              </span>
              <span className="block truncate text-[10px] text-faint">
                {t("chart.mostly", { artist: session.topArtist })}
              </span>
            </span>
            <div className="relative h-3.5">
              {/* Recessive 24-hour track the interval is read against */}
              <div className="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-edge" />
              <div
                className={`absolute top-1/2 h-2.5 -translate-y-1/2 rounded-full transition-colors ${
                  hover === i ? "bg-accent-bright" : "bg-accent"
                }`}
                style={{
                  left: `${(startHour / DAY_HOURS) * 100}%`,
                  width: `${Math.max((width / DAY_HOURS) * 100, 1.5)}%`,
                }}
              />
            </div>
            <span className="text-right whitespace-nowrap tabular-nums text-dim">
              {t("count.songs", { n: session.songs })}
              <span className="text-faint"> · {formatSpan(session.spanMinutes, t)}</span>
            </span>
          </div>
        );
      })}
      <div className="mt-1 grid gap-3" style={{ gridTemplateColumns: SESSION_COLUMNS }}>
        <span />
        <div className="flex justify-between text-[9px] text-faint">
          {[0, 6, 12, 18, 24].map((hour) => (
            <span key={hour}>{hour}:00</span>
          ))}
        </div>
      </div>
    </div>
  );
}

function formatSpan(minutes: number, t: T): string {
  if (minutes < 60) return t("format.spanMinutes", { n: minutes });
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0
    ? t("format.spanHours", { n: hours })
    : t("format.spanHoursMinutes", { h: hours, m: rest });
}
