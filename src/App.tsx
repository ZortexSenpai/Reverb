import { useEffect } from "react";
import { AppRoutes } from "./routes/router";
import { SetupView } from "./views/setup/SetupView";
import { CenteredSpinner } from "./components/ui/Spinner";
import { cmd } from "./lib/ipc/commands";
import { hydrateFromEngine } from "./lib/playback/engineBridge";
import { startQueueSync } from "./lib/playback/queueSync";
import { restoreQueueFromSnapshot, usePlayerStore } from "./stores/playerStore";
import { usePrefsStore } from "./stores/prefsStore";
import { useUiStore } from "./stores/uiStore";

export default function App() {
  const session = useUiStore((s) => s.session);
  const setSession = useUiStore((s) => s.setSession);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const status = await cmd("auth_get_status");
        if (cancelled) return;
        if (status.configured) {
          await hydrateFromEngine();
          const settings = await cmd("settings_get");
          usePlayerStore.setState({ volume: settings.volume });
          usePrefsStore.getState().hydrate(settings);
          // Bring back last session's queue if nothing is playing (e.g. the
          // engine survived a webview reload — don't clobber it).
          if (usePlayerStore.getState().state === "stopped") {
            await restoreQueueFromSnapshot();
          }
          setSession("ready");
        } else {
          setSession("setup");
        }
      } catch {
        if (!cancelled) setSession("setup");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [setSession]);

  // After the local queue is back (and after signing in on first launch).
  useEffect(() => {
    if (session === "ready") void startQueueSync();
  }, [session]);

  if (session === "loading") return <CenteredSpinner />;
  if (session === "setup") return <SetupView />;
  return <AppRoutes />;
}
