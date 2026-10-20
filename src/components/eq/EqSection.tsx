// The parametric EQ, embedded in the Settings view. Adjustments apply live
// to the audio engine while music plays.

import { Switch } from "radix-ui";
import { Plus } from "lucide-react";
import { useEffect } from "react";
import { useT } from "../../lib/i18n";
import { useEqStore, MAX_BANDS } from "../../stores/eqStore";
import { EqBandRow } from "./EqBandRow";
import { EqCurve } from "./EqCurve";
import { EqPresetMenu } from "./EqPresetMenu";

function HydrateError() {
  const t = useT();
  const error = useEqStore((s) => s.hydrateError);
  if (!error) return null;
  return (
    <div className="mb-3 rounded-md bg-danger/15 px-3 py-2 text-xs text-danger">
      {t("eq.hydrateError", { error })}
    </div>
  );
}

export function EqSection() {
  const t = useT();
  const enabled = useEqStore((s) => s.enabled);
  const bands = useEqStore((s) => s.bands);
  const preamp = useEqStore((s) => s.preamp);
  const hydrated = useEqStore((s) => s.hydrated);
  const { setEnabled, addBand, setPreamp, hydrate } = useEqStore.getState();

  useEffect(() => {
    if (!hydrated) void hydrate();
  }, [hydrated, hydrate]);

  return (
    <section>
      <div className="flex items-center justify-between border-b border-edge px-5 py-3">
        <h2 className="text-sm font-semibold">{t("eq.title")}</h2>
        <div className="flex items-center gap-3">
          <EqPresetMenu />
          <label className="flex cursor-pointer items-center gap-2 text-xs text-dim">
            <Switch.Root
              checked={enabled}
              onCheckedChange={setEnabled}
              className="relative h-5 w-9 cursor-pointer rounded-full bg-surface-2 transition-colors data-[state=checked]:bg-eq-curve"
            >
              <Switch.Thumb className="block size-4 translate-x-0.5 rounded-full bg-text transition-transform data-[state=checked]:translate-x-[18px]" />
            </Switch.Root>
            {enabled ? t("common.on") : t("eq.bypassed")}
          </label>
        </div>
      </div>

      <div className="px-5 pt-4">
        <HydrateError />
        <EqCurve />
      </div>

      <div className="flex flex-col gap-0.5 px-4 pt-3 pb-2">
        {bands.map((band, i) => (
          <EqBandRow key={i} band={band} index={i} />
        ))}
        {bands.length < MAX_BANDS && (
          <button
            onClick={() => addBand()}
            className="mx-2 mt-1 flex cursor-pointer items-center gap-2 self-start rounded-md px-2.5 py-1.5 text-xs text-dim transition-colors hover:bg-surface-2 hover:text-text"
          >
            <Plus className="size-3.5" /> {t("eq.addBand")}
          </button>
        )}
        {bands.length === 0 && (
          <p className="px-2.5 pb-2 text-xs text-faint">{t("eq.emptyHint")}</p>
        )}
      </div>

      <div className="flex items-center gap-3 border-t border-edge px-5 py-3 text-xs text-dim">
        <span>{t("eq.headroom")}</span>
        <label className="flex cursor-pointer items-center gap-1.5">
          <input
            type="radio"
            name="preamp"
            checked={preamp.mode === "auto"}
            onChange={() => setPreamp({ mode: "auto" })}
            className="accent-(--color-eq-curve)"
          />
          {t("eq.autoPreamp")}
        </label>
        <label className="flex cursor-pointer items-center gap-1.5">
          <input
            type="radio"
            name="preamp"
            checked={preamp.mode === "manual"}
            onChange={() => setPreamp({ mode: "manual", db: 0 })}
            className="accent-(--color-eq-curve)"
          />
          {t("eq.manual")}
        </label>
        {preamp.mode === "manual" && (
          <div className="flex items-center gap-2">
            <input
              type="range"
              min={-12}
              max={12}
              step={0.5}
              value={preamp.db}
              onChange={(e) => setPreamp({ mode: "manual", db: Number(e.target.value) })}
              className="w-36 accent-(--color-eq-curve)"
            />
            <span className="w-14 tabular-nums">
              {preamp.db > 0 ? "+" : ""}
              {preamp.db.toFixed(1)} dB
            </span>
          </div>
        )}
      </div>
    </section>
  );
}
