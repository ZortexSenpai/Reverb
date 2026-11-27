import {
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
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
  Pencil,
  Plus,
  Radio,
  RefreshCw,
  Server,
  ShieldAlert,
  Trash2,
  Unplug,
  Volume2,
  X,
} from "lucide-react";
import { nanoid } from "nanoid";
import { Dialog } from "radix-ui";
import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router";
import { EQ_SECTION_ID, EqSection } from "../../components/eq/EqSection";
import { NAV_ITEMS } from "../../components/layout/navItems";
import { Button, IconButton } from "../../components/ui/Button";
import { Input } from "../../components/ui/Input";
import { MultiSelect } from "../../components/ui/MultiSelect";
import { ConfirmDialog } from "../../components/ui/Dialogs";
import {
  activeDeviceConfig,
  canGoExclusive,
  deleteDeviceConfig,
  deviceLabel,
  isConnected,
  saveDeviceConfig,
} from "../../lib/deviceConfigs";
import { formatSampleRate } from "../../lib/format";
import { LANGUAGE_NAMES, LOCALES, useT, type MessageKey, type T } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { markStatsImportStarted } from "../../lib/playback/engineBridge";
import {
  SCROBBLING_SECTION_ID,
  SERVICE_NAMES,
  SERVICES,
  SERVICES_QUERY_KEY,
  STATS_SECTION_ID,
  statsReadsFrom,
  useServices,
} from "../../lib/scrobbleServices";
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
  AccountRole,
  AccountStatus,
  DeviceConfig,
  GridSize,
  NowPlayingAlign,
  NowPlayingBackdrop,
  OutputDevice,
  ReplayGainMode,
  SeekBarStyle,
  SeekBarThickness,
  ServiceId,
  ServicesStatus,
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
      // Not the default "submit": a switch inside a form must not submit it.
      type="button"
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

      <SettingRow label={t("settings.saveQueue")} hint={t("settings.saveQueueHint")}>
        <Toggle checked={prefs.saveQueue} onChange={(saveQueue) => set({ saveQueue })} />
      </SettingRow>

      <SettingRow label={t("settings.syncQueue")} hint={t("settings.syncQueueHint")}>
        <Toggle checked={prefs.syncQueue} onChange={(syncQueue) => set({ syncQueue })} />
      </SettingRow>
    </SectionCard>
  );
}

/** A config that does not exist yet, for the "New config" dialog. */
function blankDeviceConfig(): DeviceConfig {
  return { id: nanoid(), name: "", device: null, matchSourceRate: false, exclusiveOutput: false };
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
  const deviceList = devices.data;
  const active = activeDeviceConfig(prefs);

  // The config open in the editor, and whether it is a new one.
  const [editing, setEditing] = useState<{ config: DeviceConfig; isNew: boolean } | null>(null);
  const [deleting, setDeleting] = useState<DeviceConfig | null>(null);

  function summary(config: DeviceConfig): { text: string; missing: boolean } {
    const missing = !isConnected(deviceList, config.device);
    const device = missing
      ? t("settings.deviceNotConnected")
      : (deviceLabel(deviceList, config.device) ?? t("settings.systemDefault"));
    const parts = [
      device,
      config.exclusiveOutput ? t("settings.configExclusive") : t("settings.configShared"),
    ];
    if (config.matchSourceRate) parts.push(t("settings.configMatchRate"));
    return { text: parts.join(" · "), missing };
  }

  return (
    <SectionCard icon={Volume2} title={t("settings.audio")}>
      <div className="flex items-start justify-between gap-6 py-2">
        <div className="min-w-0">
          <div className="text-sm">{t("settings.deviceConfigs")}</div>
          <div className="mt-0.5 text-xs text-faint">{t("settings.deviceConfigsHint")}</div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <IconButton
            label={t("settings.refreshDevices")}
            disabled={devices.isFetching}
            onClick={() => void devices.refetch()}
          >
            <RefreshCw className={`size-4 ${devices.isFetching ? "animate-spin" : ""}`} />
          </IconButton>
          <Button
            variant="outline"
            onClick={() => setEditing({ config: blankDeviceConfig(), isNew: true })}
          >
            <Plus className="size-4" /> {t("settings.newDeviceConfig")}
          </Button>
        </div>
      </div>

      <div
        role="radiogroup"
        aria-label={t("settings.deviceConfigs")}
        className="mt-1 mb-2 divide-y divide-edge overflow-hidden rounded-lg border border-edge"
      >
        <DeviceConfigRow
          name={t("settings.systemDefault")}
          summary={t("settings.systemDefaultConfigHint")}
          active={active == null}
          onSelect={() => set({ activeDeviceConfig: null })}
        />
        {prefs.deviceConfigs.map((config) => {
          const { text, missing } = summary(config);
          return (
            <DeviceConfigRow
              key={config.id}
              name={config.name}
              summary={text}
              missing={missing}
              active={active?.id === config.id}
              onSelect={() => set({ activeDeviceConfig: config.id })}
              onEdit={() => setEditing({ config, isNew: false })}
              onDelete={() => setDeleting(config)}
            />
          );
        })}
      </div>

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
      <p className="mt-1 text-xs text-faint">{t("settings.audioNote")}</p>

      <DeviceConfigDialog
        editing={editing}
        devices={devices}
        onSave={(config) => set(saveDeviceConfig(prefs, config))}
        onClose={() => setEditing(null)}
      />
      <ConfirmDialog
        open={deleting != null}
        title={t("settings.deleteDeviceConfigTitle", { name: deleting?.name ?? "" })}
        body={
          deleting != null && deleting.id === active?.id
            ? t("settings.deleteActiveDeviceConfigBody")
            : undefined
        }
        confirmLabel={t("common.delete")}
        danger
        onConfirm={() => {
          if (deleting) set(deleteDeviceConfig(prefs, deleting.id));
        }}
        onClose={() => setDeleting(null)}
      />
    </SectionCard>
  );
}

/**
 * One choice in the device config list. Picking the row makes it the active
 * setup; the built-in system default row has no edit or delete.
 */
function DeviceConfigRow({
  name,
  summary,
  missing = false,
  active,
  onSelect,
  onEdit,
  onDelete,
}: {
  name: string;
  summary: string;
  /** The config's device is not plugged in. */
  missing?: boolean;
  active: boolean;
  onSelect: () => void;
  onEdit?: () => void;
  onDelete?: () => void;
}) {
  const t = useT();
  return (
    <div
      className={`flex items-center gap-1 pr-2 transition-colors ${
        active ? "bg-surface-2" : "hover:bg-surface-2/50"
      }`}
    >
      <button
        role="radio"
        aria-checked={active}
        onClick={onSelect}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 px-3 py-2.5 text-left"
      >
        <span
          aria-hidden
          className={`flex size-4 shrink-0 items-center justify-center rounded-full border ${
            active ? "border-accent" : "border-faint"
          }`}
        >
          {active && <span className="size-2 rounded-full bg-accent" />}
        </span>
        <span className="min-w-0">
          <span className="block truncate text-sm">{name}</span>
          <span className={`mt-0.5 block text-xs ${missing ? "text-danger" : "text-faint"}`}>
            {summary}
          </span>
        </span>
      </button>
      {onEdit && (
        <IconButton label={t("settings.editDeviceConfig", { name })} onClick={onEdit}>
          <Pencil className="size-4" />
        </IconButton>
      )}
      {onDelete && (
        <IconButton
          label={t("settings.deleteDeviceConfig", { name })}
          onClick={onDelete}
          className="hover:text-danger"
        >
          <Trash2 className="size-4" />
        </IconButton>
      )}
    </div>
  );
}

/** Create or edit a device config. Saving the active one applies it at once. */
function DeviceConfigDialog({
  editing,
  devices,
  onSave,
  onClose,
}: {
  editing: { config: DeviceConfig; isNew: boolean } | null;
  devices: UseQueryResult<OutputDevice[]>;
  onSave: (config: DeviceConfig) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<DeviceConfig | null>(null);
  useEffect(() => {
    if (editing) setDraft(editing.config);
  }, [editing]);

  const deviceList = devices.data;
  const device = draft?.device ?? null;
  const exclusiveOk = canGoExclusive(deviceList, device);
  // A saved device that is unplugged is still the config's device: keep it
  // selectable rather than letting the picker silently read "System default".
  const deviceMissing = !isConnected(deviceList, device);
  const fallbackName =
    deviceLabel(deviceList, device) ??
    (device == null ? t("settings.systemDefault") : t("settings.outputDevice"));

  function update(patch: Partial<DeviceConfig>) {
    setDraft((d) => (d ? { ...d, ...patch } : d));
  }

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!draft) return;
    onSave({
      ...draft,
      name: draft.name.trim() || fallbackName,
      // Keep the summary truthful: the switch cannot be on where it means nothing.
      exclusiveOutput: draft.exclusiveOutput && exclusiveOk,
    });
    onClose();
  }

  return (
    <Dialog.Root open={editing != null} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60" />
        <Dialog.Content
          className="fixed top-1/2 left-1/2 z-50 max-h-[85vh] w-[34rem] max-w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-xl border border-edge bg-surface p-5 shadow-2xl focus:outline-none"
          aria-describedby={undefined}
          // A stray click beside the dialog must not throw away the edits:
          // it closes through ×, Cancel, Save (or Escape) only.
          onInteractOutside={(e) => e.preventDefault()}
        >
          <div className="mb-4 flex items-center justify-between gap-4">
            <Dialog.Title className="text-sm font-semibold">
              {editing?.isNew
                ? t("settings.newDeviceConfigTitle")
                : t("settings.editDeviceConfigTitle")}
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton type="button" label={t("common.close")} className="-my-2 -mr-2">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          {draft && (
            <form onSubmit={submit}>
              <label className="block">
                <span className="text-sm">{t("settings.deviceConfigName")}</span>
                <Input
                  className="mt-1.5"
                  value={draft.name}
                  onChange={(e) => update({ name: e.target.value })}
                  placeholder={fallbackName}
                  autoFocus
                  spellCheck={false}
                />
                <span className="mt-1 block text-xs text-faint">
                  {t("settings.deviceConfigNameHint")}
                </span>
              </label>

              <div className="mt-4">
                <span className="text-sm">{t("settings.outputDevice")}</span>
                <div className="mt-1.5 flex items-center gap-1">
                  <select
                    aria-label={t("settings.outputDevice")}
                    className={`${selectClass} min-w-0 flex-1`}
                    value={device ?? ""}
                    onChange={(e) => update({ device: e.target.value === "" ? null : e.target.value })}
                  >
                    <option value="">{t("settings.systemDefault")}</option>
                    {(deviceList ?? []).map((d) => (
                      <option key={d.id} value={d.id}>
                        {d.label}
                        {d.direct ? ` ${t("settings.deviceDirect")}` : ""}
                      </option>
                    ))}
                    {deviceMissing && device != null && (
                      <option value={device}>{t("settings.savedDeviceMissing")}</option>
                    )}
                  </select>
                  <IconButton
                    type="button"
                    label={t("settings.refreshDevices")}
                    disabled={devices.isFetching}
                    onClick={() => void devices.refetch()}
                  >
                    <RefreshCw className={`size-4 ${devices.isFetching ? "animate-spin" : ""}`} />
                  </IconButton>
                </div>
                <span className="mt-1 block text-xs text-faint">
                  {t("settings.outputDeviceHint")}
                </span>
              </div>

              <div className="mt-2">
                <SettingRow label={t("settings.matchRate")} hint={t("settings.matchRateHint")}>
                  <Toggle
                    checked={draft.matchSourceRate}
                    onChange={(matchSourceRate) => update({ matchSourceRate })}
                  />
                </SettingRow>
                <SettingRow
                  label={t("settings.exclusive")}
                  hint={
                    exclusiveOk ? t("settings.exclusiveHint") : t("settings.exclusiveUnavailable")
                  }
                >
                  <Toggle
                    checked={draft.exclusiveOutput && exclusiveOk}
                    disabled={!exclusiveOk}
                    onChange={(exclusiveOutput) => update({ exclusiveOutput })}
                  />
                </SettingRow>
              </div>

              <div className="mt-4 flex justify-end gap-2">
                <Button type="button" variant="outline" onClick={onClose}>
                  {t("common.cancel")}
                </Button>
                <Button type="submit">{t("common.save")}</Button>
              </div>
            </form>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
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

/** Refetch everything an account change touches. */
function invalidateServices(queryClient: QueryClient) {
  void queryClient.invalidateQueries({ queryKey: SERVICES_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: ["stats"] });
}

/** Last.fm | ListenBrainz, as a segmented control. */
function ServicePicker({
  value,
  onChange,
}: {
  value: ServiceId;
  onChange: (service: ServiceId) => void;
}) {
  const t = useT();
  return (
    <div
      role="radiogroup"
      aria-label={t("settings.service")}
      className="flex rounded-full border border-edge p-0.5"
    >
      {SERVICES.map((id) => (
        <button
          key={id}
          type="button"
          role="radio"
          aria-checked={value === id}
          onClick={() => onChange(id)}
          className={`cursor-pointer rounded-full px-3 py-1 text-sm transition-colors ${
            value === id ? "bg-surface-2 font-medium text-text" : "text-dim hover:text-text"
          }`}
        >
          {SERVICE_NAMES[id]}
        </button>
      ))}
    </div>
  );
}

/** Connect a Last.fm account: with the shared secret and password to scrobble. */
function LastfmForm({ role }: { role: AccountRole }) {
  const t = useT();
  const queryClient = useQueryClient();
  const showToast = useUiStore((s) => s.showToast);
  const [username, setUsername] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [sharedSecret, setSharedSecret] = useState("");
  const [password, setPassword] = useState("");
  const [connecting, setConnecting] = useState(false);
  const scrobbling = role === "scrobble";
  const ready =
    username.trim() && apiKey.trim() && (!scrobbling || (sharedSecret.trim() && password));

  async function connect(e: React.FormEvent) {
    e.preventDefault();
    setConnecting(true);
    try {
      await cmd("services_connect_lastfm", {
        role,
        username: username.trim(),
        apiKey: apiKey.trim(),
        sharedSecret: scrobbling ? sharedSecret.trim() : null,
        password: scrobbling ? password : null,
      });
      setUsername("");
      setApiKey("");
      setSharedSecret("");
      setPassword("");
      invalidateServices(queryClient);
      showToast(t("toast.connected", { service: SERVICE_NAMES.lastfm }));
    } catch (err) {
      showToast(errorMessage(err), "error");
    } finally {
      setConnecting(false);
    }
  }

  return (
    <form onSubmit={connect} className="flex max-w-md flex-col gap-2">
      <p className="text-xs text-faint">
        {scrobbling
          ? `${t("settings.lastfmScrobbleLead")} ${t("settings.lastfmPairHint")} ${t("settings.lastfmScrobbleIntro")}`
          : t("settings.lastfmIntro")}
      </p>
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
      {scrobbling && (
        <>
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
        </>
      )}
      <Button type="submit" disabled={!ready || connecting} className="self-start">
        {connecting
          ? t("settings.checking")
          : t("settings.connectService", { service: SERVICE_NAMES.lastfm })}
      </Button>
    </form>
  );
}

/**
 * Connect ListenBrainz, or a server that speaks its API. Given the role's
 * current ListenBrainz connection, it edits that one instead, and a blank
 * token keeps the stored one.
 */
function ListenbrainzForm({
  role,
  current,
  onDone,
}: {
  role: AccountRole;
  current?: AccountStatus | null;
  onDone?: () => void;
}) {
  const t = useT();
  const queryClient = useQueryClient();
  const showToast = useUiStore((s) => s.showToast);
  const editing = current?.service === "listenbrainz";
  const [token, setToken] = useState("");
  const [apiUrl, setApiUrl] = useState(editing ? (current?.apiUrl ?? "") : "");
  const [connecting, setConnecting] = useState(false);

  async function connect(e: React.FormEvent) {
    e.preventDefault();
    setConnecting(true);
    try {
      await cmd("services_connect_listenbrainz", {
        role,
        token: token.trim() || null,
        apiUrl: apiUrl.trim() || null,
      });
      setToken("");
      invalidateServices(queryClient);
      showToast(t("toast.connected", { service: SERVICE_NAMES.listenbrainz }));
      onDone?.();
    } catch (err) {
      showToast(errorMessage(err), "error");
    } finally {
      setConnecting(false);
    }
  }

  return (
    <form onSubmit={connect} className="flex max-w-md flex-col gap-2">
      {!editing && (
        <p className="text-xs text-faint">
          {role === "scrobble"
            ? t("settings.listenbrainzScrobbleIntro")
            : t("settings.listenbrainzStatsIntro")}
        </p>
      )}
      <Input
        type="password"
        value={token}
        onChange={(e) => setToken(e.target.value)}
        placeholder={
          editing ? t("settings.listenbrainzTokenKeep") : t("settings.listenbrainzToken")
        }
        spellCheck={false}
        autoComplete="off"
      />
      <Input
        value={apiUrl}
        onChange={(e) => setApiUrl(e.target.value)}
        placeholder={t("settings.listenbrainzServer")}
        spellCheck={false}
      />
      <p className="text-xs text-faint">
        {role === "scrobble"
          ? t("settings.listenbrainzServerHint")
          : t("settings.listenbrainzStatsServerHint")}
      </p>
      <Button
        type="submit"
        disabled={(!editing && !token.trim()) || connecting}
        className="self-start"
      >
        {connecting
          ? t("settings.checking")
          : editing
            ? t("common.save")
            : t("settings.connectService", { service: SERVICE_NAMES.listenbrainz })}
      </Button>
    </form>
  );
}

/** Disconnect button plus its confirmation. */
function DisconnectButton({
  role,
  service,
  body,
}: {
  role: AccountRole;
  service: ServiceId;
  body: string;
}) {
  const t = useT();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="outline" onClick={() => setOpen(true)}>
        <Unplug className="size-4" /> {t("settings.disconnect")}
      </Button>
      <ConfirmDialog
        open={open}
        title={t("settings.disconnectTitle", { service: SERVICE_NAMES[service] })}
        body={body}
        confirmLabel={t("settings.disconnect")}
        danger
        onConfirm={() => {
          void cmd("services_disconnect", { role }).then(() => invalidateServices(queryClient));
        }}
        onClose={() => setOpen(false)}
      />
    </>
  );
}

/**
 * One role's account. Shows the connection when the picked service is the
 * connected one; otherwise the form to connect the picked service, which
 * replaces the current account (`replaceNote` says what that does).
 */
function AccountPanel({
  role,
  account,
  service,
  replaceNote,
  disconnectBody,
}: {
  role: AccountRole;
  account: AccountStatus | null;
  service: ServiceId;
  replaceNote: string;
  disconnectBody: string;
}) {
  const t = useT();
  const [editing, setEditing] = useState(false);

  if (!account || account.service !== service) {
    return (
      <>
        {account && <p className="mb-2 max-w-md text-xs text-accent">{replaceNote}</p>}
        {service === "lastfm" ? <LastfmForm role={role} /> : <ListenbrainzForm role={role} />}
      </>
    );
  }

  return (
    <>
      <div className="text-sm">
        {account.username ? (
          <>
            <span className="text-dim">{t("settings.connectedAs")} </span>
            <span className="font-medium">{account.username}</span>
          </>
        ) : (
          <span className="text-dim">{t("settings.connected")}</span>
        )}
      </div>
      {account.apiUrl && (
        <div className="mt-1 truncate text-xs text-faint">
          {t("settings.listenbrainzVia", { url: account.apiUrl })}
        </div>
      )}
      {!account.history && (
        <div className="mt-1 text-xs text-faint">{t("settings.listenbrainzNoHistory")}</div>
      )}
      <div className="mt-3 flex flex-wrap gap-2">
        {account.service === "listenbrainz" && (
          <Button variant="outline" onClick={() => setEditing((open) => !open)}>
            <Server className="size-4" /> {t("settings.listenbrainzChange")}
          </Button>
        )}
        <DisconnectButton role={role} service={account.service} body={disconnectBody} />
      </div>
      {editing && (
        <div className="mt-3">
          <ListenbrainzForm role={role} current={account} onDone={() => setEditing(false)} />
        </div>
      )}
    </>
  );
}

function ScrobblingSection() {
  const t = useT();
  const prefs = usePrefsStore((s) => s.prefs);
  const set = usePrefsStore((s) => s.set);
  const { data } = useServices();
  const account = data?.scrobble ?? null;
  /** The picker's choice, until the user makes one: the connected service. */
  const [picked, setPicked] = useState<ServiceId | null>(null);
  const service = picked ?? account?.service ?? "lastfm";
  // Named after the account plays actually go to, not the picker's choice.
  const name = SERVICE_NAMES[account?.service ?? service];
  // The Stats page reads this account, so changing it changes theirs too.
  const statsFollow = Boolean(data?.statsSameAsScrobble && account?.history);

  return (
    <SectionCard icon={Radio} title={t("settings.scrobbling")}>
      <SettingRow label={t("settings.scrobble")} hint={t("settings.scrobbleHint")}>
        <Toggle
          checked={prefs.scrobbleEnabled}
          onChange={(scrobbleEnabled) => set({ scrobbleEnabled })}
        />
      </SettingRow>
      {prefs.scrobbleEnabled && (
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
      )}

      <SettingRow label={t("settings.service")} hint={t("settings.scrobbleServiceHint")}>
        <ServicePicker value={service} onChange={setPicked} />
      </SettingRow>
      <div className="py-2">
        <AccountPanel
          role="scrobble"
          account={account}
          service={service}
          replaceNote={
            statsFollow
              ? t("settings.scrobbleReplacesStats", { current: name })
              : t("settings.scrobbleReplaces", { current: name })
          }
          disconnectBody={
            statsFollow
              ? t("settings.disconnectScrobbleKeepsStats", { service: name })
              : t("settings.disconnectScrobbleBody", { service: name })
          }
        />
      </div>

      {prefs.scrobbleEnabled && (
        <>
          <SettingRow
            label={t("settings.directScrobble", { service: name })}
            hint={
              account
                ? t("settings.directScrobbleHint", { service: name })
                : t("settings.directScrobbleUnavailable")
            }
          >
            <Toggle
              checked={prefs.directScrobble && Boolean(account)}
              disabled={!account}
              onChange={(directScrobble) => set({ directScrobble })}
            />
          </SettingRow>
          {/* Shown whether or not the mode is currently on: "turn off on
              restart" means the switch above is off every time settings is
              opened fresh, and its two companions would blink out with it. */}
          <SettingRow
            label={t("settings.directScrobbleReset")}
            hint={t("settings.directScrobbleResetHint")}
          >
            <Toggle
              checked={prefs.directScrobbleResetOnStart}
              disabled={!account}
              onChange={(directScrobbleResetOnStart) => set({ directScrobbleResetOnStart })}
            />
          </SettingRow>
          <SettingRow
            label={t("settings.directScrobblePin")}
            hint={t("settings.directScrobblePinHint")}
          >
            <Toggle
              checked={prefs.directScrobblePinned}
              disabled={!account}
              onChange={(directScrobblePinned) => set({ directScrobblePinned })}
            />
          </SettingRow>
          {/* Only bites on the plays Reverb sends to the service itself, i.e.
              in the mode above — what the server forwards is the server's own
              setting. Grouped here for that reason. */}
          <SettingRow
            label={t("settings.scrobbleFirstArtist")}
            hint={
              account
                ? t("settings.scrobbleFirstArtistHint", { service: name })
                : t("settings.directScrobbleUnavailable")
            }
          >
            <Toggle
              checked={prefs.scrobbleFirstArtistOnly && Boolean(account)}
              disabled={!account}
              onChange={(scrobbleFirstArtistOnly) => set({ scrobbleFirstArtistOnly })}
            />
          </SettingRow>
        </>
      )}
    </SectionCard>
  );
}

/** How much history there is, and the button to pull more. */
function HistoryStatus({ status, source }: { status: ServicesStatus; source: ServiceId }) {
  const t = useT();
  const statsImport = useUiStore((s) => s.statsImport);
  // Progress + completion toasts come from the global stats:import listener.
  const importing = statsImport !== null || status.importing;
  const progress = !statsImport
    ? null
    : statsImport.total
      ? t("settings.importingOfTotal", { n: statsImport.imported, total: statsImport.total })
      : t("settings.importingCount", { n: statsImport.imported });

  return (
    <div className="mt-2 border-t border-edge pt-4">
      <div className="text-xs text-faint">
        {t("settings.scrobblesImported", { n: status.trackCount })}
        {status.lastSyncAt
          ? t("settings.lastSync", {
              date: t.d(status.lastSyncAt, { dateStyle: "medium", timeStyle: "short" }),
            })
          : ""}
      </div>
      {progress && <div className="mt-2 text-xs text-accent">{progress}</div>}
      <Button
        variant="outline"
        className="mt-3"
        disabled={importing}
        onClick={() => {
          markStatsImportStarted(source);
          void cmd("stats_import");
        }}
      >
        <RefreshCw className={`size-4 ${importing ? "animate-spin" : ""}`} />
        {importing ? t("settings.importing") : t("settings.importNew")}
      </Button>
    </div>
  );
}

function StatsSection() {
  const t = useT();
  const queryClient = useQueryClient();
  const showToast = useUiStore((s) => s.showToast);
  const { data } = useServices();
  const same = data?.statsSameAsScrobble ?? true;
  const scrobble = data?.scrobble ?? null;
  const own = data?.statsAccount ?? null;
  const [picked, setPicked] = useState<ServiceId | null>(null);
  const service = picked ?? own?.service ?? "lastfm";
  const reads = statsReadsFrom(data);

  function setSame(value: boolean) {
    cmd("services_set_stats_same_as_scrobble", { same: value }).then(
      () => invalidateServices(queryClient),
      (err: unknown) => showToast(errorMessage(err), "error"),
    );
  }

  return (
    <SectionCard icon={BarChart3} title={t("settings.statsSection")}>
      <SettingRow
        label={t("settings.statsSameAsScrobble")}
        hint={t("settings.statsSameAsScrobbleHint")}
      >
        <Toggle checked={same} onChange={setSame} />
      </SettingRow>
      {same ? (
        <p className="py-2 text-xs text-faint">
          {!scrobble
            ? t("settings.statsNoScrobbleAccount")
            : scrobble.history
              ? t("settings.statsFollows", {
                  service: SERVICE_NAMES[scrobble.service],
                  username: scrobble.username,
                })
              : t("settings.statsScrobbleNoHistory")}
        </p>
      ) : (
        <>
          <SettingRow label={t("settings.service")} hint={t("settings.statsServiceHint")}>
            <ServicePicker value={service} onChange={setPicked} />
          </SettingRow>
          <div className="py-2">
            <AccountPanel
              role="stats"
              account={own}
              service={service}
              replaceNote={t("settings.statsReplaces", {
                current: own ? SERVICE_NAMES[own.service] : "",
              })}
              disconnectBody={t("settings.disconnectBody", {
                service: SERVICE_NAMES[own?.service ?? service],
              })}
            />
          </div>
        </>
      )}
      {reads && data && <HistoryStatus status={data} source={reads.service} />}
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
  const location = useLocation();
  const scrollRef = useRef<HTMLDivElement>(null);

  // Jump to the section the URL's hash names (the EQ buttons link to #eq).
  // Keyed on the location key as well, so following the same link again
  // re-scrolls after the user has scrolled away. Only this pane scrolls:
  // scrollIntoView would also shift the app shell's overflow-hidden ancestors.
  useEffect(() => {
    const pane = scrollRef.current;
    const target = location.hash ? document.getElementById(location.hash.slice(1)) : null;
    if (!pane || !target || !pane.contains(target)) return;
    pane.scrollTop += target.getBoundingClientRect().top - pane.getBoundingClientRect().top;
  }, [location.key, location.hash]);

  return (
    <div className="flex h-full flex-col">
      <div className="px-6 py-4">
        <h1 className="text-2xl font-semibold">{t("nav.settings")}</h1>
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-6 pb-8">
        <AppearanceSection />
        <FullScreenPlayerSection />
        <PlaybackSection />
        <AudioSection />
        <div id={EQ_SECTION_ID} className="mt-4">
          <EqSection />
        </div>
        <div id={SCROBBLING_SECTION_ID} className="mt-4">
          <ScrobblingSection />
        </div>
        <div id={STATS_SECTION_ID} className="mt-4">
          <StatsSection />
        </div>
        <ServerSection />
        <AboutSection />
      </div>
    </div>
  );
}
