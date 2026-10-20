// Roon-style signal path: a quality dot that opens the actual audio chain
// (source → DSP → output) as reported live by the Rust engine.

import { Popover } from "radix-ui";
import { ArrowDownUp, AudioWaveform, Gauge, SlidersVertical, Speaker } from "lucide-react";
import type { ReactNode } from "react";
import { formatSampleRate } from "../../lib/format";
import { useT, type MessageKey, type T } from "../../lib/i18n";
import { usePlayerStore } from "../../stores/playerStore";

const QUALITY: Record<string, { labelKey: MessageKey; dot: string; text: string }> = {
  lossless: { labelKey: "signal.lossless", dot: "bg-violet-400", text: "text-violet-300" },
  high: { labelKey: "signal.high", dot: "bg-emerald-400", text: "text-emerald-300" },
  enhanced: { labelKey: "signal.enhanced", dot: "bg-sky-400", text: "text-sky-300" },
};

function Step({ icon, title, detail }: { icon: ReactNode; title: string; detail: string }) {
  return (
    <div className="flex items-start gap-3 px-3 py-2">
      <span className="mt-0.5 text-dim">{icon}</span>
      <div className="min-w-0">
        <div className="text-sm">{title}</div>
        <div className="text-xs text-faint">{detail}</div>
      </div>
    </div>
  );
}

export function SignalPathIndicator() {
  const t = useT();
  const path = usePlayerStore((s) => s.signalPath);
  const hasTrack = usePlayerStore((s) => s.currentTrackId !== null);

  const khz = (rate: number) => formatSampleRate(rate, t as T);

  if (!path || !hasTrack) return null;
  const quality = QUALITY[path.quality] ?? QUALITY.high;
  const qualityLabel = t(quality.labelKey);

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          aria-label={t("signal.path", { quality: qualityLabel })}
          title={t("signal.path", { quality: qualityLabel })}
          className="flex cursor-pointer items-center justify-center rounded-full p-2 transition-colors hover:bg-surface-2"
        >
          <span className={`size-2.5 rounded-full ${quality.dot} shadow-[0_0_6px] shadow-current`} />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="end"
          sideOffset={8}
          className="z-50 w-72 rounded-lg border border-edge bg-surface py-1.5 shadow-xl focus:outline-none"
        >
          <div className="flex items-center gap-2 border-b border-edge px-3 pb-2">
            <span className={`size-2 rounded-full ${quality.dot}`} />
            <span className={`text-sm font-semibold ${quality.text}`}>{qualityLabel}</span>
          </div>
          <Step
            icon={<AudioWaveform className="size-4" />}
            title={t("signal.source", { codec: path.codec ?? t("signal.unknownCodec") })}
            detail={[
              khz(path.sourceSampleRate),
              path.bitRate != null ? t("format.kbps", { n: path.bitRate }) : null,
              path.losslessSource ? t("signal.losslessTag") : t("signal.lossyTag"),
            ]
              .filter(Boolean)
              .join(" · ")}
          />
          <Step
            icon={<AudioWaveform className="size-4" />}
            title={t("signal.decode")}
            detail={t("signal.decodeDetail")}
          />
          {path.replayGainDb != null && (
            <Step
              icon={<Gauge className="size-4" />}
              title={t("signal.replayGain")}
              detail={`${path.replayGainDb > 0 ? "+" : ""}${path.replayGainDb.toFixed(1)} dB`}
            />
          )}
          {path.resampleTo != null && (
            <Step
              icon={<ArrowDownUp className="size-4" />}
              title={t("signal.resample")}
              detail={t("signal.resampleDetail", {
                from: khz(path.sourceSampleRate),
                to: khz(path.resampleTo),
              })}
            />
          )}
          {path.eqBands > 0 && (
            <Step
              icon={<SlidersVertical className="size-4" />}
              title={t("signal.eq")}
              detail={t("signal.eqDetail", { n: path.eqBands })}
            />
          )}
          {path.osResampling && path.deviceRate != null && (
            <Step
              icon={<ArrowDownUp className="size-4" />}
              title={t("signal.osMixer")}
              detail={t("signal.osMixerDetail", {
                from: khz(path.outputSampleRate),
                to: khz(path.deviceRate),
              })}
            />
          )}
          <Step
            icon={<Speaker className="size-4" />}
            title={t("signal.output", {
              device: path.outputDevice ?? t("signal.systemDefault"),
            })}
            detail={khz(path.deviceRate ?? path.outputSampleRate)}
          />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
