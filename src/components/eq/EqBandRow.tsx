import { Power, X } from "lucide-react";
import { useT, type MessageKey } from "../../lib/i18n";
import type { EqBand, EqFilterType } from "../../lib/subsonic/types";
import { useEqStore } from "../../stores/eqStore";

const TYPES: { value: EqFilterType; labelKey: MessageKey }[] = [
  { value: "peaking", labelKey: "eqType.peaking" },
  { value: "lowshelf", labelKey: "eqType.lowshelf" },
  { value: "highshelf", labelKey: "eqType.highshelf" },
  { value: "lowpass", labelKey: "eqType.lowpass" },
  { value: "highpass", labelKey: "eqType.highpass" },
];

function NumberField({
  value,
  min,
  max,
  step,
  suffix,
  onChange,
  width = "w-20",
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  suffix: string;
  onChange: (v: number) => void;
  width?: string;
}) {
  return (
    <div className={`relative ${width}`}>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (Number.isFinite(v)) onChange(Math.min(max, Math.max(min, v)));
        }}
        className="w-full rounded border border-edge bg-bg px-2 py-1 pr-8 text-right text-xs tabular-nums focus:border-accent focus:outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
      />
      <span className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-[10px] text-faint">
        {suffix}
      </span>
    </div>
  );
}

export function EqBandRow({ band, index }: { band: EqBand; index: number }) {
  const t = useT();
  const { updateBand, removeBand } = useEqStore.getState();
  const isPass = band.type === "lowpass" || band.type === "highpass";

  return (
    <div
      className={`flex items-center gap-2 rounded-md px-2 py-1.5 transition-opacity ${
        band.enabled ? "" : "opacity-45"
      }`}
    >
      <span className="w-5 text-center text-xs font-semibold text-eq-curve">{index + 1}</span>
      <button
        aria-label={band.enabled ? t("eq.disableBand") : t("eq.enableBand")}
        onClick={() => updateBand(index, { enabled: !band.enabled })}
        className={`cursor-pointer rounded-full p-1 transition-colors ${
          band.enabled ? "text-eq-curve" : "text-faint hover:text-dim"
        }`}
      >
        <Power className="size-3.5" />
      </button>
      <select
        value={band.type}
        onChange={(e) => updateBand(index, { type: e.target.value as EqFilterType })}
        className="w-24 cursor-pointer rounded border border-edge bg-bg px-1.5 py-1 text-xs focus:border-accent focus:outline-none"
      >
        {TYPES.map((type) => (
          <option key={type.value} value={type.value}>
            {t(type.labelKey)}
          </option>
        ))}
      </select>
      <NumberField
        value={Math.round(band.frequency)}
        min={20}
        max={20000}
        step={1}
        suffix="Hz"
        onChange={(frequency) => updateBand(index, { frequency })}
        width="w-24"
      />
      <NumberField
        value={Math.round(band.gainDb * 10) / 10}
        min={-18}
        max={18}
        step={0.5}
        suffix="dB"
        onChange={(gainDb) => updateBand(index, { gainDb })}
      />
      {!isPass ? null : <span className="w-0" />}
      <NumberField
        value={Math.round(band.q * 100) / 100}
        min={0.1}
        max={10}
        step={0.1}
        suffix="Q"
        onChange={(q) => updateBand(index, { q })}
      />
      <button
        aria-label={t("eq.removeBand")}
        onClick={() => removeBand(index)}
        className="ml-auto cursor-pointer rounded-full p-1 text-faint transition-colors hover:text-danger"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
