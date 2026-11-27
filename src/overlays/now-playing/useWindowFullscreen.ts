// OS-level fullscreen for the app window, owned by the full-screen player:
// it's toggled from there and dropped again when the player closes, so the
// window never stays fullscreen behind the regular layout.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useState } from "react";

export function useWindowFullscreen(active: boolean) {
  const [fullscreen, setFullscreen] = useState(false);

  useEffect(() => {
    if (!active) return;
    const win = getCurrentWindow();
    let disposed = false;
    // The OS can change it behind our back (green button, Esc on macOS), and
    // a fullscreen transition always resizes the window.
    const sync = () =>
      win.isFullscreen().then((v) => {
        if (!disposed) setFullscreen(v);
      });
    void sync();
    const unlisten = win.onResized(() => void sync());
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
      void win.isFullscreen().then((v) => {
        if (v) void win.setFullscreen(false);
      });
      setFullscreen(false);
    };
  }, [active]);

  const toggle = useCallback(() => {
    const next = !fullscreen;
    setFullscreen(next);
    void getCurrentWindow().setFullscreen(next);
  }, [fullscreen]);

  return { fullscreen, toggle };
}
