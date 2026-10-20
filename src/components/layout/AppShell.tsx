import type { CSSProperties } from "react";
import { Outlet } from "react-router";
import { NewPlaylistDialog } from "../library/NewPlaylistDialog";
import { NowPlayingOverlay } from "../../overlays/now-playing/NowPlayingOverlay";
import { QueuePanel } from "../../overlays/now-playing/QueuePanel";
import { UpdatePanel } from "../update/UpdatePanel";
import { useGlobalShortcuts } from "../../lib/keyboard";
import { themeArt } from "../../lib/themes/genshin";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";
import { PlayerBar } from "./PlayerBar";
import { Sidebar } from "./Sidebar";

function Toast() {
  const toast = useUiStore((s) => s.toast);
  if (!toast) return null;
  return (
    <div
      className={`fixed bottom-24 left-1/2 z-50 -translate-x-1/2 rounded-lg px-4 py-2 text-sm shadow-lg ${
        toast.kind === "error" ? "bg-danger text-white" : "bg-surface-2 text-text"
      }`}
    >
      {toast.message}
    </div>
  );
}

/** Ambient wash the liquid layout's glass panels are tinted by. */
function LiquidBackdrop() {
  return <div className="liquid-backdrop absolute inset-0" />;
}

/**
 * Character art behind the content area, for themes that have some. Opaque
 * chrome (classic sidebar and player bar) paints over it; the liquid layout's
 * glass shows it through, blurred.
 */
function ThemeArtLayer() {
  const art = themeArt(usePrefsStore((s) => s.prefs.theme));
  if (!art) return null;
  return (
    <div
      aria-hidden
      className={`theme-art ${art.character ? "theme-art-character" : ""}`}
      style={{ backgroundImage: `url("${art.url}")` }}
    />
  );
}

export function AppShell() {
  useGlobalShortcuts();
  const liquid = usePrefsStore((s) => s.prefs.layout) === "liquid";
  const glassIntensity = usePrefsStore((s) => s.prefs.glassIntensity);
  // 0–100 preference → the 0–1 multiplier every glass rule scales off.
  const glass = Math.min(100, Math.max(0, glassIntensity)) / 100;
  // Same for panel transparency; globals.css caps how far it thins the panels.
  const panelTransparency = usePrefsStore((s) => s.prefs.panelTransparency);
  const panelClear = Math.min(100, Math.max(0, panelTransparency)) / 100;

  const overlays = (
    <>
      <NowPlayingOverlay />
      <QueuePanel />
      <NewPlaylistDialog />
      <UpdatePanel />
      <Toast />
    </>
  );

  if (liquid) {
    return (
      <div
        className="liquid-shell relative h-full"
        style={{ "--glass": glass, "--panel-clear": panelClear } as CSSProperties}
      >
        <LiquidBackdrop />
        <ThemeArtLayer />
        {/* `relative` keeps the panels above the backdrop without a z-index
            fight — negative z-index would sink behind the body background. */}
        <div className="relative flex h-full flex-col gap-3 p-3">
          <div className="flex min-h-0 flex-1 gap-3">
            <Sidebar />
            <main className="glass min-w-0 flex-1 overflow-hidden rounded-2xl">
              <Outlet />
            </main>
          </div>
          <PlayerBar />
        </div>
        {overlays}
      </div>
    );
  }

  return (
    <div
      className="relative flex h-full flex-col"
      style={{ "--panel-clear": panelClear } as CSSProperties}
    >
      <ThemeArtLayer />
      <div className="relative flex min-h-0 flex-1">
        <Sidebar />
        <main className="min-w-0 flex-1 overflow-hidden">
          <Outlet />
        </main>
      </div>
      <div className="relative shrink-0">
        <PlayerBar />
      </div>
      {overlays}
    </div>
  );
}
