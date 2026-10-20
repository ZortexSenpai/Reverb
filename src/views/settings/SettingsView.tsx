import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getVersion } from "@tauri-apps/api/app";
import {
  BarChart3,
  Check,
  ExternalLink,
  Gauge,
  Info,
  LogOut,
  Maximize2,
  Palette,
  Radio,
  RefreshCw,
  Server,
  ShieldAlert,
  Unplug,
  Volume2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { EqSection } from "../../components/eq/EqSection";
import { NAV_ITEMS } from "../../components/layout/navItems";
import { Button, IconButton } from "../../components/ui/Button";
import { Input } from "../../components/ui/Input";
import { MultiSelect } from "../../components/ui/MultiSelect";
import { ConfirmDialog } from "../../components/ui/Dialogs";
import { formatSampleRate } from "../../lib/format";
import { LANGUAGE_NAMES, LOCALES, useT, type MessageKey, type T } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { markStatsImportStarted } from "../../lib/playback/engineBridge";
import { themeArt } from "../../lib/themes/genshin";
import {
  downloadedUpdate,
  releaseUrl,
  updateController,
  updatesEnabled,
  useUpdateState,
  versionLabel,
  type UpdateState,
} from "../../lib/updater";
import type {
  GridSize,
  NowPlayingAlign,
  NowPlayingBackdrop,
  ReplayGainMode,
  SeekBarStyle,
  SeekBarThickness,
  StreamingFormat,
} from "../../lib/subsonic/types";
import { NOW_PLAYING_FIELDS } from "../../overlays/now-playing/nowPlayingFields";
import { usePlayerStore } from "../../stores/playerStore";
import type { ThemeOption } from "../../stores/prefsStore";
import {
  ACCENT_PRESETS,
  LAYOUTS,
  NOW_PLAYING_ALIGNS,
  NOW_PLAYING_ART_SIZES,
  NOW_PLAYING_BACKDROPS,
  STARTUP_VIEWS,
  THEME_ACCENT,
  THEMES,
  usePrefsStore,
} from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";

const selectClass =
  "cursor-pointer rounded-md border border-edge bg-bg px-2.5 py-1.5 text-sm text-text focus:border-accent focus:outline-none";

function SectionCard({
  icon: Icon,
  title,
  children,
}: {
  icon: typeof Server;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="mt-4 p-5 first:mt-0">
      <h2 className="mb-4 flex items-center gap-2 text-sm font-semibold text-dim">
        <Icon className="size-4" /> {title}
      </h2>
      {children}
    </section>
  );
}

function khz(rate: number, t: T): string {
  return formatSampleRate(rate, t);
}

function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 py-2">
      <div className="min-w-0">
        <div className="text-sm">{label}</div>
        {hint && <div className="mt-0.5 text-xs text-faint">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Toggle({
  checked,
  onChange,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-5 w-9 rounded-full transition-colors ${
        disabled ? "cursor-not-allowed opacity-40" : "cursor-pointer"
      } ${checked ? "bg-accent" : "bg-surface-2"}`}
    >
      <span
        className={`absolute top-0.5 block size-4 rounded-full bg-text transition-transform ${
          checked ? "translate-x-[18px]" : "translate-x-0.5"
        }`}
      />
    </button>
  );
}

function ServerSection() {
  const t = useT();
  const setSession = useUiStore((s) => s.setSession);
  const showToast = useUiStore((s) => s.showToast);
  const queryClient = useQueryClient();
  const [scanning, setScanning] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const status = useQuery({ queryKey: ["auth-status"], queryFn: () => cmd("auth_get_status") });

  useEffect(() => () => {
    if (pollRef.current) clearInterval(pollRef.current);
  }, []);

  async function startScan() {
    setScanning(true);
    try {
      await cmd("library_start_scan");
      pollRef.current = setInterval(async () => {
        try {
          const s = await cmd("library_get_scan_status");
          if (!s.scanning) {
            if (pollRef.current) clearInterval(pollRef.current);
            setScanning(false);
            void queryClient.invalidateQueries();
            showToast(
              s.count != null
                ? t("toast.scanFinishedCount", { n: s.count })
                : t("toast.scanFinished"),
            );
          }
        } catch {
          if (pollRef.current) clearInterval(pollRef.current);
          setScanning(false);
        }
      }, 2000);
    } catch (e) {
      setScanning(false);
      showToast(e instanceof Error ? e.message : t("toast.scanFailed"), "error");
    }
  }

  async function signOut() {
    await cmd("auth_sign_out");
    usePlayerStore.getState().resetQueue();
    queryClient.clear();
    setSession("setup");
  }

  return (
    <SectionCard icon={Server} title={t("settings.server")}>
      {status.data && (
        <>
          <div className="text-sm">
            <span className="text-dim">{t("settings.connectedTo")}</span>
            <span className="font-medium">{status.data.serverUrl}</span>
            <span className="text-dim">{t("settings.asUser")}</span>
            <span className="font-medium">{status.data.username}</span>
          </div>
          {status.data.insecureStorage && (
            <div className="mt-2 flex items-start gap-2 rounded-md bg-surface-2 p-2.5 text-xs text-dim">
              <ShieldAlert className="mt-0.5 size-4 shrink-0 text-danger" />
              {t("settings.insecureStorage")}
            </div>
          )}
          <div className="mt-4 flex gap-2">
            <Button variant="outline" onClick={startScan} disabled={scanning}>
              <RefreshCw className={`size-4 ${scanning ? "animate-spin" : ""}`} />
              {scanning ? t("settings.scanning") : t("settings.scanLibrary")}
            </Button>
            <Button variant="outline" onClick={signOut}>
              <LogOut className="size-4" /> {t("settings.signOut")}
            </Button>
          </div>
        </>
      )}
    </SectionCard>
  );
}

function PlaybackSection() {
  const t = useT();
  const prefs = usePrefsStore((s) => s.prefs);
  const set = usePrefsStore((s) => s.set);
  const rg = prefs.replayGain;
  // Shares the cache entry with the stats-sources section below.
  const sources = useQuery({
    queryKey: ["stats-sources"],
    queryFn: () => cmd("stats_get_sources"),
  });
  const canScrobbleToLastfm = Boolean(sources.data?.lastfm?.scrobbling);

  return (
    <SectionCard icon={Gauge} title={t("settings.playback")}>
      <SettingRow
        label={t("settings.streamingQuality")}
        hint={t("settings.streamingQualityHint")}
      >
        <select
          className={selectClass}
          value={prefs.streamingFormat}
          onChange={(e) => set({ streamingFormat: e.target.value as StreamingFormat })}
        >
          <option value="raw">{t("quality.raw")}</option>
          <option value="mp3-320">MP3 320 kbps</option>
          <option value="mp3-192">MP3 192 kbps</option>
          <option value="mp3-128">MP3 128 kbps</option>
        </select>
      </SettingRow>

      <SettingRow label={t("settings.replayGain")} hint={t("settings.replayGainHint")}>
        <select
          className={selectClass}
          value={rg.mode}
          onChange={(e) =>
            set({ replayGain: { ...rg, mode: e.target.value as ReplayGainMode } })
          }
        >
          <option value="off">{t("common.off")}</option>
          <option value="track">{t("replayGain.track")}</option>
          <option value="album">{t("replayGain.album")}</option>
        </select>
      </SettingRow>

      {rg.mode !== "off" && (
        <>
          <SettingRow label={t("settings.replayGainPreamp")}>
            <div className="flex items-center gap-2 text-xs text-dim">
              <input
                type="range"
                min={-15}
                max={15}
                step={0.5}
                value={rg.preampDb}
                onChange={(e) =>
                  set({ replayGain: { ...rg, preampDb: Number(e.target.value) } })
                }
                className="w-36 accent-(--color-accent)"
              />
              <span className="w-16 tabular-nums">
                {rg.preampDb > 0 ? "+" : ""}
                {rg.preampDb.toFixed(1)} dB
              </span>
            </div>
          </SettingRow>
          <SettingRow label={t("settings.fallbackGain")} hint={t("settings.fallbackGainHint")}>
            <div className="flex items-center gap-2 text-xs text-dim">
              <input
                type="range"
                min={-15}
                max={0}
                step={0.5}
                value={rg.fallbackDb}
                onChange={(e) =>
                  set({ replayGain: { ...rg, fallbackDb: Number(e.target.value) } })
                }
                className="w-36 accent-(--color-accent)"
              />
              <span className="w-16 tabular-nums">{rg.fallbackDb.toFixed(1)} dB</span>
            </div>
          </SettingRow>
          <SettingRow
            label={t("settings.clipProtection")}
            hint={t("settings.clipProtectionHint")}
          >
            <Toggle
              checked={rg.clipProtection}
              onChange={(clipProtection) => set({ replayGain: { ...rg, clipProtection } })}
            />
          </SettingRow>
        </>
      )}

      <SettingRow label={t("settings.crossfade")} hint={t("settings.crossfadeHint")}>
        <div className="flex items-center gap-2 text-xs text-dim">
          <input
            type="range"
            min={0}
            max={12}
            step={1}
            value={prefs.crossfadeSecs}
            onChange={(e) => set({ crossfadeSecs: Number(e.target.value) })}
            className="w-36 accent-(--color-accent)"
          />
          <span className="w-10 tabular-nums">
            {prefs.crossfadeSecs === 0 ? t("common.off") : `${prefs.crossfadeSecs}s`}
          </span>
        </div>
      </SettingRow>

      <SettingRow label={t("settings.radio")} hint={t("settings.radioHint")}>
        <Toggle checked={prefs.radioMode} onChange={(radioMode) => set({ radioMode })} />
      </SettingRow>

      <SettingRow label={t("settings.scrobble")} hint={t("settings.scrobbleHint")}>
        <Toggle
          checked={prefs.scrobbleEnabled}
          onChange={(scrobbleEnabled) => set({ scrobbleEnabled })}
        />
      </SettingRow>

      {prefs.scrobbleEnabled && (
        <>
          <SettingRow label={t("settings.markPlayed")} hint={t("settings.markPlayedHint")}>
            <div className="flex items-center gap-2 text-xs text-dim">
              <input
                type="range"
                min={10}
                max={100}
                step={5}
                value={prefs.scrobbleAtPercent}
                onChange={(e) => set({ scrobbleAtPercent: Number(e.target.value) })}
                className="w-36 accent-(--color-accent)"
              />
              <span className="w-10 tabular-nums">{prefs.scrobbleAtPercent}%</span>
            </div>
          </SettingRow>

          <SettingRow
            label={t("settings.lastfmOnly")}
            hint={
              canScrobbleToLastfm
                ? t("settings.lastfmOnlyHint")
                : t("settings.lastfmOnlyUnavailable")
            }
          >
            <Toggle
              checked={prefs.lastfmScrobbleOnly && canScrobbleToLastfm}
              disabled={!canScrobbleToLastfm}
              onChange={(lastfmScrobbleOnly) => set({ lastfmScrobbleOnly })}
            />
          </SettingRow>
          {/* Shown whether or not the mode is currently on: "turn off on
              restart" means the switch above is off every time settings is
              opened fresh, and its two companions would blink out with it. */}
          <SettingRow
            label={t("settings.lastfmOnlyReset")}
            hint={t("settings.lastfmOnlyResetHint")}
          >
            <Toggle
              checked={prefs.lastfmScrobbleOnlyResetOnStart}
              disabled={!canScrobbleToLastfm}
              onChange={(lastfmScrobbleOnlyResetOnStart) =>
                set({ lastfmScrobbleOnlyResetOnStart })
              }
            />
          </SettingRow>
          <SettingRow label={t("settings.lastfmOnlyPin")} hint={t("settings.lastfmOnlyPinHint")}>
            <Toggle
              checked={prefs.lastfmScrobbleOnlyPinned}
              disabled={!canScrobbleToLastfm}
              onChange={(lastfmScrobbleOnlyPinned) => set({ lastfmScrobbleOnlyPinned })}
            />
          </SettingRow>
          {/* Only bites on the plays Reverb sends to Last.fm itself, i.e. in
              the mode above — what the server forwards is the server's own
              setting. Grouped here for that reason. */}
          <SettingRow
            label={t("settings.lastfmFirstArtist")}
            hint={
              canScrobbleToLastfm
                ? t("settings.lastfmFirstArtistHint")
                : t("settings.lastfmOnlyUnavailable")
            }
          >
            <Toggle
              checked={prefs.lastfmScrobbleFirstArtistOnly && canScrobbleToLastfm}
              disabled={!canScrobbleToLastfm}
              onChange={(lastfmScrobbleFirstArtistOnly) =>
                set({ lastfmScrobbleFirstArtistOnly })
              }
            />
          </SettingRow>
        </>
      )}

      <SettingRow label={t("settings.saveQueue")} hint={t("settings.saveQueueHint")}>
        <Toggle checked={prefs.saveQueue} onChange={(saveQueue) => set({ saveQueue })} />
      </SettingRow>

      <SettingRow label={t("settings.syncQueue")} hint={t("settings.syncQueueHint")}>
        <Toggle checked={prefs.syncQueue} onChange={(syncQueue) => set({ syncQueue })} />
      </SettingRow>
    </SectionCard>
  );
}

function AudioSection() {
  const t = useT();
  const prefs = usePrefsStore((s) => s.prefs);
  const set = usePrefsStore((s) => s.set);
  const sampleRate = usePlayerStore((s) => s.sampleRate);
  // The stream rate proves nothing about the hardware, so prefer the rate the
  // engine confirmed from the device — which is only readable while a stream is
  // open, hence the fallback below.
  const deviceRate = usePlayerStore((s) => s.signalPath?.deviceRate ?? null);
  const devices = useQuery({ queryKey: ["audio-devices"], queryFn: () => cmd("audio_list_devices") });

  // Exclusive access only means something on Linux once a direct (hw:) device
  // is picked: a sound-server route has no card to take, so the engine's
  // exclusive path resolves to the shared default anyway. On macOS and Windows
  // it is a device-level mode that works on the default device too, and no
  // device is ever reported as direct — so only gate where the flag exists.
  const deviceList = devices.data;
  const selectedDevice = deviceList?.find((d) => d.id === prefs.audioDevice);
  const canGoExclusive =
    // Still loading: leave the toggle alone rather than blinking it disabled.
    deviceList == null || !deviceList.some((d) => d.direct) || Boolean(selectedDevice?.direct);

  return (
    <SectionCard icon={Volume2} title={t("settings.audio")}>
      <SettingRow label={t("settings.outputDevice")} hint={t("settings.outputDeviceHint")}>
        <div className="flex items-center gap-1">
          <select
            className={`${selectClass} max-w-72`}
            value={prefs.audioDevice ?? ""}
            onChange={(e) => set({ audioDevice: e.target.value === "" ? null : e.target.value })}
          >
            <option value="">{t("settings.systemDefault")}</option>
            {(devices.data ?? []).map((device) => (
              <option key={device.id} value={device.id}>
                {device.label}
                {device.direct ? ` ${t("settings.deviceDirect")}` : ""}
              </option>
            ))}
          </select>
          <IconButton
            label={t("settings.refreshDevices")}
            disabled={devices.isFetching}
            onClick={() => void devices.refetch()}
          >
            <RefreshCw className={`size-4 ${devices.isFetching ? "animate-spin" : ""}`} />
          </IconButton>
        </div>
      </SettingRow>
      <SettingRow
        label={t("settings.deviceRate")}
        hint={
          deviceRate == null
            ? t("settings.deviceRateStreamHint")
            : deviceRate !== sampleRate
              ? t("settings.deviceRateConvertedHint", { rate: khz(sampleRate, t) })
              : t("settings.deviceRateConfirmedHint")
        }
      >
        <span className="text-sm font-medium">
          {khz(deviceRate ?? sampleRate, t)}
          {deviceRate == null && (
            <span className="text-faint"> {t("settings.streamTag")}</span>
          )}
        </span>
      </SettingRow>
      <SettingRow
        label={t("settings.matchRate")}
        hint={t("settings.matchRateHint")}
      >
        <Toggle
          checked={prefs.matchSourceRate}
          onChange={(matchSourceRate) => set({ matchSourceRate })}
        />
      </SettingRow>
      <SettingRow
        label={t("settings.exclusive")}
        hint={
          canGoExclusive ? t("settings.exclusiveHint") : t("settings.exclusiveUnavailable")
        }
      >
        <Toggle
          checked={prefs.exclusiveOutput && canGoExclusive}
          disabled={!canGoExclusive}
          onChange={(exclusiveOutput) => set({ exclusiveOutput })}
        />
      </SettingRow>
      <p className="mt-1 text-xs text-faint">{t("settings.audioNote")}</p>
    </SectionCard>
  );
}

/**
 * A theme's colours in miniature: a window with a sidebar, two lines of text
 * and an accent. `data-theme` on the wrapper is what makes it work — the theme
 * blocks in globals.css match any element with the attribute, so the tokens
 * inside resolve to that palette while the rest of the page keeps its own.
 */
function ThemeSwatch({ theme }: { theme: string }) {
  const art = themeArt(theme);
  return (
    <span data-theme={theme} className="relative flex h-20 gap-1.5 overflow-hidden rounded-md bg-bg p-2">
      <span className="w-1/6 rounded bg-surface-2" />
      <span className="flex flex-1 flex-col gap-1.5 rounded bg-surface p-2">
        <span className="h-1 w-3/5 rounded-full bg-text" />
        <span className="h-1 w-2/5 rounded-full bg-dim" />
        <span className="mt-auto h-1.5 w-1/3 rounded-full bg-accent" />
      </span>
      {art && (
        <span
          aria-hidden
          className="pointer-events-none absolute inset-y-0 right-0 w-3/5 bg-contain bg-right-bottom bg-no-repeat"
          style={{ backgroundImage: `url("${art.url}")` }}
        />
      )}
    </span>
  );
}

function ThemeCard({
  theme,
  selected,
  onSelect,
}: {
  theme: ThemeOption;
  selected: boolean;
  onSelect: () => void;
}) {
  const t = useT();
  return (
    <button
      onClick={onSelect}
      aria-pressed={selected}
      className={`cursor-pointer rounded-lg border p-1.5 text-left transition-colors ${
        selected ? "border-accent bg-surface-2" : "border-edge hover:border-faint hover:bg-surface-2/50"
      }`}
    >
      {/* "Follow system" has no palette of its own — show both outcomes. */}
      {theme.automatic ? (
        <span className="flex h-20 gap-1 overflow-hidden rounded-md">
          <span className="flex-1">
            <ThemeSwatch theme="light" />
          </span>
          <span className="flex-1">
            <ThemeSwatch theme="dark" />
          </span>
        </span>
      ) : (
        <ThemeSwatch theme={theme.value} />
      )}
      <span className="mt-1.5 flex items-center gap-1">
        <span className="truncate text-xs">
          {theme.labelKey ? t(theme.labelKey) : theme.label}
        </span>
        {selected && <Check className="ml-auto size-3 shrink-0 text-accent" />}
      </span>
    </button>
  );
}

function ThemePicker({
  value,
  onSelect,
}: {
  value: string;
  onSelect: (theme: string) => void;
}) {
  const t = useT();
  const groups = [
    {
      title: t("settings.themeGroupAutomatic"),
      themes: THEMES.filter((theme) => theme.automatic),
    },
    {
      title: t("settings.themeGroupDark"),
      themes: THEMES.filter((theme) => !theme.automatic && !theme.light && !theme.genshin),
    },
    {
      title: t("settings.themeGroupLight"),
      themes: THEMES.filter((theme) => theme.light && !theme.genshin),
    },
    { title: t("settings.themeGroupGenshin"), themes: THEMES.filter((theme) => theme.genshin) },
  ];

  return (
    <div className="mt-3 flex flex-col gap-4">
      {groups.map((group) => (
        <div key={group.title}>
          <div className="mb-1.5 text-[10px] font-semibold tracking-wide text-faint uppercase">
            {group.title}
          </div>
          <div
            className="grid gap-2"
            style={{ gridTemplateColumns: "repeat(auto-fill, minmax(9.5rem, 1fr))" }}
          >
            {group.themes.map((theme) => (
              <ThemeCard
                key={theme.value}
                theme={theme}
                selected={value === theme.value}
                onSelect={() => onSelect(theme.value)}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function AppearanceSection() {
  const t = useT();
  const prefs = usePrefsStore((s) => s.prefs);
  const set = usePrefsStore((s) => s.set);

  return (
    <SectionCard icon={Palette} title={t("settings.appearance")}>
      <SettingRow label={t("settings.language")} hint={t("settings.languageHint")}>
        <select
          className={selectClass}
          value={prefs.language}
          onChange={(e) => set({ language: e.target.value })}
        >
          <option value="system">{t("language.system")}</option>
          {/* Language names stay in their own language, as a picker should:
              someone looking for their language reads it, not its exonym. */}
          {LOCALES.map((locale) => (
            <option key={locale} value={locale}>
              {LANGUAGE_NAMES[locale]}
            </option>
          ))}
        </select>
      </SettingRow>
      <div className="py-2">
        <div className="text-sm">{t("settings.theme")}</div>
        <div className="mt-0.5 text-xs text-faint">{t("settings.themeHint")}</div>
        <ThemePicker value={prefs.theme} onSelect={(theme) => set({ theme })} />
      </div>
      <SettingRow label={t("settings.accent")} hint={t("settings.accentHint")}>
        <div className="flex items-center gap-2">
          <button
            aria-label={t("settings.accentNamed", { name: t("settings.accentMatchTheme") })}
            title={t("settings.accentMatchTheme")}
            onClick={() => set({ accent: THEME_ACCENT })}
            className={`flex size-6 cursor-pointer items-center justify-center rounded-full border border-edge bg-surface-2 text-dim transition-transform hover:scale-110 ${
              prefs.accent === THEME_ACCENT
                ? "ring-2 ring-text ring-offset-2 ring-offset-surface"
                : ""
            }`}
          >
            <Palette className="size-3.5" />
          </button>
          {Object.entries(ACCENT_PRESETS).map(([name, preset]) => (
            <button
              key={name}
              aria-label={t("settings.accentNamed", { name: t(`accent.${name}` as MessageKey) })}
              title={t(`accent.${name}` as MessageKey)}
              onClick={() => set({ accent: name })}
              className={`size-6 cursor-pointer rounded-full transition-transform hover:scale-110 ${
                prefs.accent === name ? "ring-2 ring-text ring-offset-2 ring-offset-surface" : ""
              }`}
              style={{ backgroundColor: preset.accent }}
            />
          ))}
        </div>
      </SettingRow>
      <SettingRow
        label={t("settings.layout")}
        hint={(() => {
          const hintKey = LAYOUTS.find((l) => l.value === prefs.layout)?.hintKey;
          return hintKey && t(hintKey);
        })()}
      >
        <div className="flex items-center gap-1 rounded-lg bg-bg p-1">
          {LAYOUTS.map((l) => (
            <button
              key={l.value}
              onClick={() => set({ layout: l.value })}
              aria-pressed={prefs.layout === l.value}
              className={`cursor-pointer rounded-md px-3 py-1.5 text-sm transition-colors ${
                prefs.layout === l.value
                  ? "bg-accent font-medium text-white"
                  : "text-dim hover:bg-surface-2 hover:text-text"
              }`}
            >
              {t(l.labelKey)}
            </button>
          ))}
        </div>
      </SettingRow>
      {prefs.layout === "liquid" && (
        <SettingRow label={t("settings.glass")} hint={t("settings.glassHint")}>
          <div className="flex items-center gap-2 text-xs text-dim">
            <input
              type="range"
              min={0}
              max={100}
              step={5}
              value={prefs.glassIntensity}
              onChange={(e) => set({ glassIntensity: Number(e.target.value) })}
              className="w-36 accent-(--color-accent)"
            />
            <span className="w-10 tabular-nums">
              {prefs.glassIntensity === 0 ? t("common.off") : `${prefs.glassIntensity}%`}
            </span>
          </div>
        </SettingRow>
      )}
      <SettingRow
        label={t("settings.panelTransparency")}
        hint={t("settings.panelTransparencyHint")}
      >
        <div className="flex items-center gap-2 text-xs text-dim">
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={prefs.panelTransparency}
            onChange={(e) => set({ panelTransparency: Number(e.target.value) })}
            className="w-36 accent-(--color-accent)"
          />
          <span className="w-10 tabular-nums">
            {prefs.panelTransparency === 0 ? t("common.off") : `${prefs.panelTransparency}%`}
          </span>
        </div>
      </SettingRow>
      <SettingRow label={t("settings.startupView")}>
        <select
          className={selectClass}
          value={prefs.startupView}
          onChange={(e) => set({ startupView: e.target.value })}
        >
          {STARTUP_VIEWS.map((v) => (
            <option key={v.value} value={v.value}>
              {t(v.labelKey)}
            </option>
          ))}
        </select>
      </SettingRow>
      <SettingRow label={t("settings.seekBar")} hint={t("settings.seekBarHint")}>
        <select
          className={selectClass}
          value={prefs.seekBarStyle}
          onChange={(e) => set({ seekBarStyle: e.target.value as SeekBarStyle })}
        >
          <option value="line">{t("seekBar.line")}</option>
          <option value="wave">{t("seekBar.wave")}</option>
          <option value="bars">{t("seekBar.bars")}</option>
        </select>
      </SettingRow>
      <SettingRow label={t("settings.seekBarThickness")}>
        <select
          className={selectClass}
          value={prefs.seekBarThickness}
          onChange={(e) => set({ seekBarThickness: e.target.value as SeekBarThickness })}
        >
          <option value="thin">{t("seekBar.thin")}</option>
          <option value="medium">{t("size.medium")}</option>
          <option value="thick">{t("seekBar.thick")}</option>
        </select>
      </SettingRow>
      {/* Only the wave and bars move, so the switch would be inert on a line. */}
      {prefs.seekBarStyle !== "line" && (
        <SettingRow
          label={t("settings.seekBarAnimated")}
          hint={t("settings.seekBarAnimatedHint")}
        >
          <Toggle
            checked={prefs.seekBarAnimated}
            onChange={(seekBarAnimated) => set({ seekBarAnimated })}
          />
        </SettingRow>
      )}
      <SettingRow label={t("settings.gridDensity")} hint={t("settings.gridDensityHint")}>
        <select
          className={selectClass}
          value={prefs.gridSize}
          onChange={(e) => set({ gridSize: e.target.value as GridSize })}
        >
          <option value="small">{t("density.compact")}</option>
          <option value="medium">{t("density.comfortable")}</option>
          <option value="large">{t("density.spacious")}</option>
        </select>
      </SettingRow>
      <SettingRow label={t("settings.sidebarItems")} hint={t("settings.sidebarItemsHint")}>
        {/* Selected means shown; the preference stores the complement so that
            navigation added in a later release is visible by default. */}
        <MultiSelect
          label={t("settings.sidebarItems")}
          options={NAV_ITEMS.map((item) => ({
            value: item.to,
            label: t(item.labelKey),
            icon: item.icon,
          }))}
          selected={NAV_ITEMS.filter((item) => !prefs.hiddenNavItems.includes(item.to)).map(
            (item) => item.to,
          )}
          onChange={(shown) =>
            set({
              hiddenNavItems: NAV_ITEMS.filter((item) => !shown.includes(item.to)).map(
                (item) => item.to,
              ),
            })
          }
        />
      </SettingRow>
      <SettingRow label={t("settings.pinnedSidebar")} hint={t("settings.pinnedSidebarHint")}>
        <Toggle
          checked={prefs.showPinnedInSidebar}
          onChange={(showPinnedInSidebar) => set({ showPinnedInSidebar })}
        />
      </SettingRow>
    </SectionCard>
  );
}

function FullScreenPlayerSection() {
  const t = useT();
  const prefs = usePrefsStore((s) => s.prefs);
  const set = usePrefsStore((s) => s.set);
  const backdropHintKey = NOW_PLAYING_BACKDROPS.find(
    (b) => b.value === prefs.nowPlayingBackdrop,
  )?.hintKey;
  const alignHintKey = NOW_PLAYING_ALIGNS.find((a) => a.value === prefs.nowPlayingAlign)?.hintKey;

  return (
    <SectionCard icon={Maximize2} title={t("settings.fullScreenPlayer")}>
      <SettingRow
        label={t("settings.backdrop")}
        hint={t(backdropHintKey ?? "settings.backdropHint")}
      >
        <select
          className={selectClass}
          value={prefs.nowPlayingBackdrop}
          onChange={(e) => set({ nowPlayingBackdrop: e.target.value as NowPlayingBackdrop })}
        >
          {NOW_PLAYING_BACKDROPS.map((b) => (
            <option key={b.value} value={b.value}>
              {t(b.labelKey)}
            </option>
          ))}
        </select>
      </SettingRow>
      <SettingRow label={t("settings.coverSize")} hint={t("settings.coverSizeHint")}>
        <div className="flex items-center gap-1 rounded-lg bg-bg p-1">
          {NOW_PLAYING_ART_SIZES.map((s) => (
            <button
              key={s.value}
              onClick={() => set({ nowPlayingArtSize: s.value })}
              aria-pressed={prefs.nowPlayingArtSize === s.value}
              className={`cursor-pointer rounded-md px-3 py-1.5 text-sm transition-colors ${
                prefs.nowPlayingArtSize === s.value
                  ? "bg-accent font-medium text-white"
                  : "text-dim hover:bg-surface-2 hover:text-text"
              }`}
            >
              {t(s.labelKey)}
            </button>
          ))}
        </div>
      </SettingRow>
      <SettingRow
        label={t("settings.textAlign")}
        hint={alignHintKey && t(alignHintKey)}
      >
        <select
          className={selectClass}
          value={prefs.nowPlayingAlign}
          onChange={(e) => set({ nowPlayingAlign: e.target.value as NowPlayingAlign })}
        >
          {NOW_PLAYING_ALIGNS.map((a) => (
            <option key={a.value} value={a.value}>
              {t(a.labelKey)}
            </option>
          ))}
        </select>
      </SettingRow>
      <SettingRow label={t("settings.genreBubbles")} hint={t("settings.genreBubblesHint")}>
        <Toggle
          checked={prefs.nowPlayingGenres}
          onChange={(nowPlayingGenres) => set({ nowPlayingGenres })}
        />
      </SettingRow>
      <SettingRow label={t("settings.detailsShown")} hint={t("settings.detailsShownHint")}>
        <MultiSelect
          label={t("settings.detailsShown")}
          options={NOW_PLAYING_FIELDS.map((f) => ({ value: f.id, label: t(f.labelKey) }))}
          selected={prefs.nowPlayingFields}
          onChange={(nowPlayingFields) => set({ nowPlayingFields })}
        />
      </SettingRow>
    </SectionCard>
  );
}

function StatsSourcesSection() {
  const t = useT();
  const queryClient = useQueryClient();
  const showToast = useUiStore((s) => s.showToast);
  const statsImport = useUiStore((s) => s.statsImport);
  const [username, setUsername] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [sharedSecret, setSharedSecret] = useState("");
  const [password, setPassword] = useState("");
  const [connecting, setConnecting] = useState(false);
  const [disconnectOpen, setDisconnectOpen] = useState(false);
  /** The add-scrobbling form, shown for an already-connected source. */
  const [scrobbleFormOpen, setScrobbleFormOpen] = useState(false);

  const sources = useQuery({
    queryKey: ["stats-sources"],
    queryFn: () => cmd("stats_get_sources"),
  });

  // Progress + completion toasts come from the global stats:import listener.
  const importing = statsImport !== null || Boolean(sources.data?.importing);
  const progress = !statsImport
    ? null
    : statsImport.total
      ? t("settings.importingOfTotal", { n: statsImport.imported, total: statsImport.total })
      : t("settings.importingCount", { n: statsImport.imported });

  async function connect(e: React.FormEvent) {
    e.preventDefault();
    setConnecting(true);
    try {
      await cmd("stats_configure_lastfm", {
        username: username.trim(),
        apiKey: apiKey.trim(),
        sharedSecret: sharedSecret.trim() || null,
        password: password || null,
      });
      setUsername("");
      setApiKey("");
      setSharedSecret("");
      setPassword("");
      void queryClient.invalidateQueries({ queryKey: ["stats-sources"] });
      markStatsImportStarted();
      void cmd("stats_import", { source: "lastfm" });
      showToast(t("toast.lastfmConnected"));
    } catch (err) {
      showToast(errorMessage(err), "error");
    } finally {
      setConnecting(false);
    }
  }

  /** Add scrobbling to a source that is already connected for imports. */
  async function enableScrobbling(e: React.FormEvent) {
    e.preventDefault();
    setConnecting(true);
    try {
      await cmd("stats_enable_lastfm_scrobbling", {
        sharedSecret: sharedSecret.trim(),
        password,
        apiKey: apiKey.trim() || null,
      });
      setSharedSecret("");
      setPassword("");
      setApiKey("");
      setScrobbleFormOpen(false);
      void queryClient.invalidateQueries({ queryKey: ["stats-sources"] });
      showToast(t("toast.lastfmScrobblingOn"));
    } catch (err) {
      showToast(errorMessage(err), "error");
    } finally {
      setConnecting(false);
    }
  }

  const lastfm = sources.data?.lastfm;

  return (
    <SectionCard icon={BarChart3} title={t("settings.statsSources")}>
      {lastfm ? (
        <>
          <div className="text-sm">
            <span className="font-medium">Last.fm</span>
            <span className="text-dim"> {t("settings.lastfmConnectedAs")} </span>
            <span className="font-medium">{lastfm.username}</span>
            {lastfm.scrobbling && (
              <span className="text-accent">{t("settings.lastfmScrobblingOn")}</span>
            )}
          </div>
          {!lastfm.scrobbling && (
            <div className="mt-1 text-xs text-faint">{t("settings.lastfmImportOnly")}</div>
          )}
          <div className="mt-1 text-xs text-faint">
            {t("settings.scrobblesImported", { n: lastfm.trackCount })}
            {lastfm.lastSyncAt
              ? t("settings.lastSync", {
                  date: t.d(lastfm.lastSyncAt, { dateStyle: "medium", timeStyle: "short" }),
                })
              : ""}
          </div>
          {progress && <div className="mt-2 text-xs text-accent">{progress}</div>}
          <div className="mt-3 flex gap-2">
            <Button
              variant="outline"
              disabled={importing}
              onClick={() => {
                markStatsImportStarted();
                void cmd("stats_import", { source: "lastfm" });
              }}
            >
              <RefreshCw className={`size-4 ${importing ? "animate-spin" : ""}`} />
              {importing ? t("settings.importing") : t("settings.importNew")}
            </Button>
            {!lastfm.scrobbling && (
              <Button variant="outline" onClick={() => setScrobbleFormOpen((open) => !open)}>
                <Radio className="size-4" /> {t("settings.setUpScrobbling")}
              </Button>
            )}
            <Button variant="outline" onClick={() => setDisconnectOpen(true)}>
              <Unplug className="size-4" /> {t("settings.disconnect")}
            </Button>
          </div>
          {scrobbleFormOpen && !lastfm.scrobbling && (
            <form onSubmit={enableScrobbling} className="mt-3 flex max-w-md flex-col gap-2">
              <p className="text-xs text-faint">{t("settings.lastfmPairHint")}</p>
              <Input
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={t("settings.lastfmApiKeyReplace")}
                spellCheck={false}
              />
              <Input
                value={sharedSecret}
                onChange={(e) => setSharedSecret(e.target.value)}
                placeholder={t("settings.lastfmSharedSecret")}
                spellCheck={false}
              />
              <Input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={t("settings.lastfmPassword")}
              />
              <Button
                type="submit"
                disabled={!sharedSecret.trim() || !password || connecting}
                className="self-start"
              >
                {connecting ? t("settings.checking") : t("settings.setUpScrobbling")}
              </Button>
            </form>
          )}
          <ConfirmDialog
            open={disconnectOpen}
            title={t("settings.disconnectTitle")}
            body={t("settings.disconnectBody")}
            confirmLabel={t("settings.disconnect")}
            danger
            onConfirm={() => {
              void cmd("stats_disconnect", { source: "lastfm" }).then(() => {
                void queryClient.invalidateQueries({ queryKey: ["stats-sources"] });
                void queryClient.invalidateQueries({ queryKey: ["stats"] });
              });
            }}
            onClose={() => setDisconnectOpen(false)}
          />
        </>
      ) : (
        <form onSubmit={connect} className="flex max-w-md flex-col gap-2">
          <p className="text-xs text-faint">{t("settings.lastfmIntro")}</p>
          <Input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder={t("settings.lastfmUsername")}
            spellCheck={false}
          />
          <Input
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={t("settings.apiKey")}
            spellCheck={false}
          />
          <p className="mt-1 text-xs text-faint">
            {t("settings.optionalForScrobbling")} {t("settings.lastfmPairHint")}{" "}
            {t("settings.lastfmScrobbleIntro")}
          </p>
          <Input
            value={sharedSecret}
            onChange={(e) => setSharedSecret(e.target.value)}
            placeholder={t("settings.lastfmSharedSecret")}
            spellCheck={false}
          />
          <Input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={t("settings.lastfmPassword")}
          />
          <Button
            type="submit"
            disabled={!username.trim() || !apiKey.trim() || connecting}
            className="self-start"
          >
            {connecting ? t("settings.checking") : t("settings.connectLastfm")}
          </Button>
        </form>
      )}
    </SectionCard>
  );
}

/** What the last update check came to, for the line under "Check now". */
function updateStatus(state: UpdateState, t: T): string | undefined {
  switch (state.status) {
    case "idle":
      return undefined;
    case "checking":
      return t("about.checking");
    case "downloading":
      return t("about.downloading", { version: versionLabel(state.update.version) });
    case "ready":
      return t("about.ready", { version: versionLabel(state.update.version) });
    case "installing":
      return t("about.installing");
    case "upToDate":
      return t("about.upToDate");
    case "error":
      return state.during === "check"
        ? t("about.checkFailed", { error: state.message })
        : t("update.installFailed", { error: state.message });
  }
}

function AboutSection() {
  const t = useT();
  const autoCheck = usePrefsStore((s) => s.prefs.checkForUpdates);
  const set = usePrefsStore((s) => s.set);
  const openUpdatePanel = useUiStore((s) => s.setUpdatePanelOpen);
  const update = useUpdateState();
  const version = useQuery({ queryKey: ["app-version"], queryFn: getVersion, staleTime: Infinity });
  const busy = ["checking", "downloading", "installing"].includes(update.status);

  return (
    <SectionCard icon={Info} title={t("settings.about")}>
      <SettingRow label={t("about.version")} hint={t("about.versionHint")}>
        {version.data && (
          <a
            href={releaseUrl(version.data)}
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-1 text-sm font-medium tabular-nums hover:underline"
          >
            {versionLabel(version.data)} <ExternalLink className="size-3.5 text-faint" />
          </a>
        )}
      </SettingRow>
      <SettingRow label={t("about.autoCheck")} hint={t("about.autoCheckHint")}>
        <Toggle checked={autoCheck} onChange={(checkForUpdates) => set({ checkForUpdates })} />
      </SettingRow>
      <SettingRow
        label={t("about.updates")}
        hint={updatesEnabled ? updateStatus(update, t) : t("about.devBuild")}
      >
        <div className="flex gap-2">
          {/* Also after a failed install, so it can be retried from here. */}
          {downloadedUpdate(update) && (
            <Button variant="outline" onClick={() => openUpdatePanel(true)}>
              {t("about.showUpdate")}
            </Button>
          )}
          <Button
            variant="outline"
            onClick={() => void updateController.checkNow()}
            disabled={busy || !updatesEnabled}
          >
            <RefreshCw className={`size-4 ${busy ? "animate-spin" : ""}`} />
            {t("about.checkNow")}
          </Button>
        </div>
      </SettingRow>
    </SectionCard>
  );
}

export function SettingsView() {
  const t = useT();
  return (
    <div className="flex h-full flex-col">
      <div className="px-6 py-4">
        <h1 className="text-2xl font-semibold">{t("nav.settings")}</h1>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-8">
        <AppearanceSection />
        <FullScreenPlayerSection />
        <PlaybackSection />
        <AudioSection />
        <div className="mt-4">
          <EqSection />
        </div>
        <StatsSourcesSection />
        <ServerSection />
        <AboutSection />
      </div>
    </div>
  );
}
