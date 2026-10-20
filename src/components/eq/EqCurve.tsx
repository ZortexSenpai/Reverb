// Interactive frequency-response plot: log-x 20Hz–20kHz, ±18dB y.
// Drag a handle = frequency + gain. Mouse wheel over a handle = Q.
// Click empty space = add a band there. Double-click a handle = toggle band.

import { useMemo, useRef } from "react";
import {
  compositeResponseDb,
  designBiquad,
  logFrequencies,
  magnitudeDb,
} from "../../lib/audio/biquad";
import { MAX_BANDS, useEqStore } from "../../stores/eqStore";
import { usePlayerStore } from "../../stores/playerStore";

const W = 840;
const H = 280;
const PAD_L = 36;
const PAD_R = 12;
const PAD_T = 10;
const PAD_B = 24;
const DB_RANGE = 18;
const CURVE_POINTS = 200;

const FREQ_GRID = [50, 100, 200, 500, 1000, 2000, 5000, 10000];
const DB_GRID = [-12, -6, 0, 6, 12];

function xOf(freq: number): number {
  const t = (Math.log10(freq) - Math.log10(20)) / (Math.log10(20000) - Math.log10(20));
  return PAD_L + t * (W - PAD_L - PAD_R);
}

function freqOf(x: number): number {
  const t = (x - PAD_L) / (W - PAD_L - PAD_R);
  const f = Math.pow(10, Math.log10(20) + t * (Math.log10(20000) - Math.log10(20)));
  return Math.min(20000, Math.max(20, f));
}

function yOf(db: number): number {
  const t = (db + DB_RANGE) / (2 * DB_RANGE);
  return H - PAD_B - t * (H - PAD_T - PAD_B);
}

function dbOf(y: number): number {
  const t = (H - PAD_B - y) / (H - PAD_T - PAD_B);
  return Math.min(DB_RANGE, Math.max(-DB_RANGE, t * 2 * DB_RANGE - DB_RANGE));
}

function fmtFreq(f: number): string {
  return f >= 1000 ? `${(f / 1000).toFixed(f >= 10000 ? 0 : 1)}k` : `${Math.round(f)}`;
}

export function EqCurve() {
  const bands = useEqStore((s) => s.bands);
  const enabled = useEqStore((s) => s.enabled);
  const { updateBand, addBand, setDragging } = useEqStore.getState();
  const sampleRate = usePlayerStore((s) => s.sampleRate);
  const svgRef = useRef<SVGSVGElement>(null);
  const dragIndex = useRef<number | null>(null);

  const freqs = useMemo(() => logFrequencies(CURVE_POINTS), []);

  const bandPaths = useMemo(
    () =>
      bands.map((band) => {
        if (!band.enabled) return "";
        const c = designBiquad(band, sampleRate);
        return freqs
          .map(
            (f, i) =>
              `${i === 0 ? "M" : "L"}${xOf(f).toFixed(1)},${yOf(
                Math.min(DB_RANGE, Math.max(-DB_RANGE, magnitudeDb(c, f, sampleRate))),
              ).toFixed(1)}`,
          )
          .join("");
      }),
    [bands, freqs, sampleRate],
  );

  const compositePath = useMemo(() => {
    const response = compositeResponseDb(bands, freqs, sampleRate);
    return freqs
      .map(
        (f, i) =>
          `${i === 0 ? "M" : "L"}${xOf(f).toFixed(1)},${yOf(
            Math.min(DB_RANGE, Math.max(-DB_RANGE, response[i])),
          ).toFixed(1)}`,
      )
      .join("");
  }, [bands, freqs, sampleRate]);

  function svgPoint(e: React.PointerEvent | React.MouseEvent): { x: number; y: number } {
    const svg = svgRef.current!;
    const rect = svg.getBoundingClientRect();
    return {
      x: ((e.clientX - rect.left) / rect.width) * W,
      y: ((e.clientY - rect.top) / rect.height) * H,
    };
  }

  function onHandleDown(index: number, e: React.PointerEvent) {
    e.stopPropagation();
    (e.target as Element).setPointerCapture(e.pointerId);
    dragIndex.current = index;
    setDragging(true);
  }

  function onHandleMove(e: React.PointerEvent) {
    const index = dragIndex.current;
    if (index == null) return;
    const { x, y } = svgPoint(e);
    const band = useEqStore.getState().bands[index];
    if (!band) return;
    const isPass = band.type === "lowpass" || band.type === "highpass";
    updateBand(index, {
      frequency: Math.round(freqOf(x)),
      ...(isPass ? {} : { gainDb: Math.round(dbOf(y) * 10) / 10 }),
    });
  }

  function onHandleUp() {
    dragIndex.current = null;
    setDragging(false);
  }

  function onHandleWheel(index: number, e: React.WheelEvent) {
    const band = useEqStore.getState().bands[index];
    if (!band) return;
    const factor = e.deltaY > 0 ? 1 / 1.12 : 1.12;
    updateBand(index, {
      q: Math.round(Math.min(10, Math.max(0.1, band.q * factor)) * 100) / 100,
    });
  }

  function onBackgroundClick(e: React.MouseEvent) {
    if (bands.length >= MAX_BANDS) return;
    const { x, y } = svgPoint(e);
    if (x < PAD_L || x > W - PAD_R || y < PAD_T || y > H - PAD_B) return;
    addBand(Math.round(freqOf(x)), Math.round(dbOf(y) * 10) / 10);
  }

  return (
    <svg
      ref={svgRef}
      viewBox={`0 0 ${W} ${H}`}
      className={`w-full cursor-crosshair select-none rounded-lg bg-surface/60 ${enabled ? "" : "opacity-40"} transition-opacity`}
      onClick={onBackgroundClick}
      onPointerMove={onHandleMove}
      onPointerUp={onHandleUp}
    >
      {/* grid */}
      {FREQ_GRID.map((f) => (
        <g key={f}>
          <line
            x1={xOf(f)}
            x2={xOf(f)}
            y1={PAD_T}
            y2={H - PAD_B}
            className="stroke-edge"
            strokeWidth={0.5}
          />
          <text
            x={xOf(f)}
            y={H - 8}
            textAnchor="middle"
            className="fill-faint text-[10px]"
          >
            {fmtFreq(f)}
          </text>
        </g>
      ))}
      {DB_GRID.map((db) => (
        <g key={db}>
          <line
            x1={PAD_L}
            x2={W - PAD_R}
            y1={yOf(db)}
            y2={yOf(db)}
            className={db === 0 ? "stroke-dim" : "stroke-edge"}
            strokeWidth={db === 0 ? 1 : 0.5}
          />
          <text
            x={PAD_L - 6}
            y={yOf(db) + 3}
            textAnchor="end"
            className="fill-faint text-[10px]"
          >
            {db > 0 ? `+${db}` : db}
          </text>
        </g>
      ))}

      {/* per-band faint curves */}
      {bandPaths.map((d, i) =>
        d ? (
          <path key={i} d={d} fill="none" className="stroke-eq-curve/25" strokeWidth={1.2} />
        ) : null,
      )}

      {/* composite curve */}
      <path d={compositePath} fill="none" className="stroke-eq-curve" strokeWidth={2.2} />

      {/* handles */}
      {bands.map((band, i) => {
        const isPass = band.type === "lowpass" || band.type === "highpass";
        const cx = xOf(Math.min(20000, Math.max(20, band.frequency)));
        const cy = yOf(isPass ? 0 : band.gainDb);
        return (
          <g key={i} className={band.enabled ? "" : "opacity-35"}>
            <circle
              cx={cx}
              cy={cy}
              r={9}
              className="cursor-grab fill-surface stroke-eq-curve active:cursor-grabbing"
              strokeWidth={2}
              onClick={(e) => e.stopPropagation()}
              onPointerDown={(e) => onHandleDown(i, e)}
              onWheel={(e) => onHandleWheel(i, e)}
              onDoubleClick={(e) => {
                e.stopPropagation();
                updateBand(i, { enabled: !band.enabled });
              }}
            />
            <text
              x={cx}
              y={cy + 3.5}
              textAnchor="middle"
              className="pointer-events-none fill-text text-[10px] font-semibold"
            >
              {i + 1}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
