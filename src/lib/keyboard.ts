// App-wide keyboard shortcuts. Mounted once in AppShell.

import { useEffect } from "react";
import { useNavigate } from "react-router";
import { usePlayerStore } from "../stores/playerStore";
import { useUiStore } from "../stores/uiStore";
import { positionClock } from "./playback/positionClock";

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  );
}

export function useGlobalShortcuts() {
  const navigate = useNavigate();

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (isTypingTarget(e.target)) return;
      const player = usePlayerStore.getState();
      const ui = useUiStore.getState();
      const mod = e.ctrlKey || e.metaKey;

      switch (e.key) {
        case " ":
          e.preventDefault();
          player.togglePlay();
          break;
        case "ArrowRight":
          e.preventDefault();
          if (mod) player.next();
          else player.seekMs(positionClock.now() + 10_000);
          break;
        case "ArrowLeft":
          e.preventDefault();
          if (mod) player.previous();
          else player.seekMs(Math.max(0, positionClock.now() - 10_000));
          break;
        case "f":
        case "F":
          if (!mod) ui.setNowPlayingOpen(!ui.nowPlayingOpen);
          break;
        case "e":
        case "E":
          if (mod) {
            e.preventDefault();
            navigate("/settings");
          }
          break;
        case "m":
        case "M":
          if (!mod) player.toggleMute();
          break;
        case "s":
        case "S":
          if (!mod) player.toggleShuffle();
          break;
        case "r":
        case "R":
          if (!mod) player.cycleRepeat();
          break;
        case "k":
        case "K":
          if (mod) {
            e.preventDefault();
            navigate("/search");
          }
          break;
        case "/":
          e.preventDefault();
          navigate("/search");
          break;
        case "Escape":
          if (ui.nowPlayingOpen) ui.setNowPlayingOpen(false);
          else if (ui.queuePanelOpen) ui.setQueuePanelOpen(false);
          break;
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate]);
}
