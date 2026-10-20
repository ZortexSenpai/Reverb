// User preferences: mirrored from settings.json, mutated through
// settings_set_prefs. Accent color is applied to CSS custom properties here.

import { create } from "zustand";
import { cmd } from "../lib/ipc/commands";
import { resolveLocale, type MessageKey } from "../lib/i18n/core";
import { GENSHIN_THEMES } from "../lib/themes/genshin";
import type {
  LayoutMode,
  NowPlayingAlign,
  NowPlayingArtSize,
  NowPlayingBackdrop,
  Preferences,
} from "../lib/subsonic/types";

export const ACCENT_PRESETS: Record<string, { accent: string; bright: string }> = {
  coral: { accent: "oklch(0.72 0.17 20)", bright: "oklch(0.78 0.17 20)" },
  blue: { accent: "oklch(0.66 0.15 250)", bright: "oklch(0.72 0.15 250)" },
  green: { accent: "oklch(0.7 0.14 155)", bright: "oklch(0.76 0.14 155)" },
  purple: { accent: "oklch(0.66 0.17 300)", bright: "oklch(0.72 0.17 300)" },
  amber: { accent: "oklch(0.75 0.14 75)", bright: "oklch(0.81 0.14 75)" },
};

/**
 * `light: true` flips `color-scheme` (native widgets) and the glass tint, and
 * decides which half of the theme picker a palette lands in. Labels name the
 * family in full, since the picker lists them in one grid rather than grouped
 * dropdown sections. Every value needs a matching `[data-theme]` block in
 * globals.css — the picker previews render from those same tokens.
 */
export interface ThemeOption {
  value: string;
  /**
   * The palette's own name. Most are the upstream project's, which stays as
   * written in every language; the generic ones carry a `labelKey` instead.
   */
  label: string;
  labelKey?: MessageKey;
  light?: boolean;
  /** Resolves to light or dark at runtime; shown apart from the two halves. */
  automatic?: boolean;
  /** Character themes with background art; the picker gives them their own section. */
  genshin?: boolean;
}

export const THEMES: ThemeOption[] = [
  { value: "system", label: "Follow system", labelKey: "theme.system", automatic: true },
  { value: "dark", label: "Dark", labelKey: "theme.dark" },
  { value: "black", label: "Black (OLED)", labelKey: "theme.black" },
  { value: "light", label: "Light", labelKey: "theme.light", light: true },
  { value: "catppuccin-mocha", label: "Catppuccin Mocha" },
  { value: "catppuccin-macchiato", label: "Catppuccin Macchiato" },
  { value: "catppuccin-frappe", label: "Catppuccin Frappé" },
  { value: "catppuccin-latte", label: "Catppuccin Latte", light: true },
  { value: "dracula", label: "Dracula" },
  { value: "everforest-dark", label: "Everforest Dark" },
  { value: "everforest-light", label: "Everforest Light", light: true },
  { value: "github-dark", label: "GitHub Dark" },
  { value: "github-light", label: "GitHub Light", light: true },
  { value: "gruvbox-dark", label: "Gruvbox Dark" },
  { value: "gruvbox-light", label: "Gruvbox Light", light: true },
  { value: "kanagawa", label: "Kanagawa" },
  { value: "monokai-pro", label: "Monokai Pro" },
  { value: "nord", label: "Nord" },
  { value: "nord-light", label: "Nord Light", light: true },
  { value: "one-dark", label: "One Dark" },
  { value: "one-light", label: "One Light", light: true },
  { value: "rose-pine", label: "Rosé Pine" },
  { value: "rose-pine-moon", label: "Rosé Pine Moon" },
  { value: "rose-pine-dawn", label: "Rosé Pine Dawn", light: true },
  { value: "solarized-dark", label: "Solarized Dark" },
  { value: "solarized-light", label: "Solarized Light", light: true },
  { value: "tokyo-night", label: "Tokyo Night" },
  { value: "tokyo-night-storm", label: "Tokyo Night Storm" },
  ...GENSHIN_THEMES.map(({ value, label, light }) => ({ value, label, light, genshin: true })),
];

const LIGHT_THEMES = new Set(THEMES.filter((t) => t.light).map((t) => t.value));

/** Accent value meaning "whatever the current theme declares". */
export const THEME_ACCENT = "theme";

export const LAYOUTS: { value: LayoutMode; labelKey: MessageKey; hintKey: MessageKey }[] = [
  { value: "classic", labelKey: "layout.classic", hintKey: "layout.classicHint" },
  { value: "liquid", labelKey: "layout.liquid", hintKey: "layout.liquidHint" },
];

export const NOW_PLAYING_BACKDROPS: {
  value: NowPlayingBackdrop;
  labelKey: MessageKey;
  hintKey: MessageKey;
}[] = [
  { value: "blurred", labelKey: "backdrop.blurred", hintKey: "backdrop.blurredHint" },
  { value: "soft", labelKey: "backdrop.soft", hintKey: "backdrop.softHint" },
  { value: "clear", labelKey: "backdrop.clear", hintKey: "backdrop.clearHint" },
  { value: "solid", labelKey: "backdrop.solid", hintKey: "backdrop.solidHint" },
];

export const NOW_PLAYING_ART_SIZES: { value: NowPlayingArtSize; labelKey: MessageKey }[] = [
  { value: "small", labelKey: "size.small" },
  { value: "medium", labelKey: "size.medium" },
  { value: "large", labelKey: "size.large" },
];

export const NOW_PLAYING_ALIGNS: {
  value: NowPlayingAlign;
  labelKey: MessageKey;
  hintKey: MessageKey;
}[] = [
  { value: "center", labelKey: "align.center", hintKey: "align.centerHint" },
  { value: "left", labelKey: "align.left", hintKey: "align.leftHint" },
];

export const STARTUP_VIEWS: { value: string; labelKey: MessageKey }[] = [
  { value: "/home", labelKey: "nav.home" },
  { value: "/albums", labelKey: "nav.albums" },
  { value: "/songs", labelKey: "nav.songs" },
  { value: "/artists", labelKey: "nav.artists" },
  { value: "/genres", labelKey: "nav.genres" },
  { value: "/folders", labelKey: "nav.folders" },
  { value: "/playlists", labelKey: "nav.playlists" },
  { value: "/favourites", labelKey: "nav.favourites" },
  { value: "/stats", labelKey: "nav.stats" },
];

export const GRID_CARD_WIDTH: Record<string, number> = {
  small: 150,
  medium: 190,
  large: 240,
};

const DEFAULT_PREFS: Preferences = {
  theme: "dark",
  language: "system",
  accent: "coral",
  layout: "classic",
  glassIntensity: 100,
  panelTransparency: 0,
  nowPlayingBackdrop: "blurred",
  nowPlayingArtSize: "medium",
  nowPlayingAlign: "center",
  nowPlayingFields: ["format", "bitRate", "sampleRate"],
  nowPlayingGenres: true,
  startupView: "/home",
  gridSize: "medium",
  playlistLayout: "grid",
  audioDevice: null,
  matchSourceRate: false,
  exclusiveOutput: false,
  streamingFormat: "raw",
  replayGain: { mode: "off", preampDb: 0, clipProtection: true, fallbackDb: 0 },
  crossfadeSecs: 0,
  seekBarStyle: "line",
  seekBarAnimated: true,
  seekBarThickness: "medium",
  scrobbleEnabled: true,
  scrobbleAtPercent: 80,
  lastfmScrobbleOnly: false,
  lastfmScrobbleOnlyResetOnStart: false,
  lastfmScrobbleOnlyPinned: false,
  lastfmScrobbleFirstArtistOnly: false,
  songColumns: ["artist", "album", "duration"],
  pinnedPlaylists: [],
  showPinnedInSidebar: true,
  hiddenNavItems: [],
  radioMode: false,
  saveQueue: true,
  syncQueue: true,
  checkForUpdates: true,
};

function applyAccent(accent: string) {
  const root = document.documentElement.style;
  const preset = ACCENT_PRESETS[accent];
  if (!preset) {
    // "Match theme" (and any unknown value): drop the inline overrides so the
    // accent the theme declares in CSS shows through, and keeps showing
    // through when the theme changes.
    root.removeProperty("--color-accent");
    root.removeProperty("--color-accent-bright");
    return;
  }
  root.setProperty("--color-accent", preset.accent);
  root.setProperty("--color-accent-bright", preset.bright);
}

/**
 * Stamp the resolved locale on <html>. The UI reads the preference through
 * `useT`; this is for everything that reads the document instead — hyphenation
 * and font fallback in the engine, and assistive tech announcing the page.
 */
function applyLanguage(language: string) {
  document.documentElement.lang = resolveLocale(language);
}

/** Stamp the theme on <html>; globals.css keys its token overrides off it. */
function applyTheme(theme: string) {
  const resolved =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : theme;
  const root = document.documentElement;
  root.dataset.theme = resolved;
  root.dataset.scheme = LIGHT_THEMES.has(resolved) ? "light" : "dark";
}

// Re-resolve "system" when the OS appearance flips. Guarded because this module
// is now reachable from plain unit tests (via the i18n hooks), which run without
// a DOM.
if (typeof window !== "undefined") {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (usePrefsStore.getState().prefs.theme === "system") applyTheme("system");
  });
}

interface PrefsStore {
  prefs: Preferences;
  hydrated: boolean;

  /** Adopt settings loaded at boot (missing fields fall back to defaults). */
  hydrate: (raw: Partial<Preferences>) => void;
  /** Update one or more preferences, persist, and apply side effects. */
  set: (patch: Partial<Preferences>) => void;
  togglePinnedPlaylist: (id: string) => void;
}

export const usePrefsStore = create<PrefsStore>((set, get) => ({
  prefs: DEFAULT_PREFS,
  hydrated: false,

  hydrate: (raw) => {
    const prefs: Preferences = {
      ...DEFAULT_PREFS,
      ...Object.fromEntries(Object.entries(raw).filter(([, v]) => v !== undefined)),
      replayGain: { ...DEFAULT_PREFS.replayGain, ...(raw.replayGain ?? {}) },
    };
    set({ prefs, hydrated: true });
    applyAccent(prefs.accent);
    applyTheme(prefs.theme);
    applyLanguage(prefs.language);
  },

  set: (patch) => {
    const prefs = { ...get().prefs, ...patch };
    set({ prefs });
    if (patch.accent) applyAccent(prefs.accent);
    if (patch.theme) applyTheme(prefs.theme);
    if (patch.language) applyLanguage(prefs.language);
    void cmd("settings_set_prefs", { prefs });
  },

  togglePinnedPlaylist: (id) => {
    const pinned = get().prefs.pinnedPlaylists;
    get().set({
      pinnedPlaylists: pinned.includes(id)
        ? pinned.filter((p) => p !== id)
        : [...pinned, id],
    });
  },
}));
