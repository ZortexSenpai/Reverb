// The downloaded update: what changed, a link to the release, and the one
// button that installs it. Opened from the update marker or Settings → About.

import { ExternalLink, RotateCw } from "lucide-react";
import { Dialog } from "radix-ui";
import { useT } from "../../lib/i18n";
import { releaseUrl, updateController, useUpdateState, type UpdateState } from "../../lib/updater";
import { useUiStore } from "../../stores/uiStore";
import { Button } from "../ui/Button";
import { ReleaseNotes } from "./ReleaseNotes";

/** The update the panel is about: only one that has finished downloading. */
function downloadedUpdate(state: UpdateState) {
  switch (state.status) {
    case "ready":
    case "installing":
      return state.update;
    case "error":
      return state.during === "install" ? state.update : null;
    default:
      return null;
  }
}

export function UpdatePanel() {
  const t = useT();
  const state = useUpdateState();
  const open = useUiStore((s) => s.updatePanelOpen);
  const setOpen = useUiStore((s) => s.setUpdatePanelOpen);
  const update = downloadedUpdate(state);
  const installing = state.status === "installing";
  const released = update?.date && !Number.isNaN(Date.parse(update.date)) ? update.date : null;

  return (
    <Dialog.Root
      open={open && update != null}
      onOpenChange={(o) => !o && !installing && setOpen(false)}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60" />
        {update && (
          <Dialog.Content
            className="fixed top-1/2 left-1/2 z-50 flex max-h-[80vh] w-[32rem] max-w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col rounded-xl border border-edge bg-surface p-5 shadow-2xl focus:outline-none"
            aria-describedby={undefined}
          >
            <Dialog.Title className="text-base font-semibold">
              {t("update.title", { version: `v${update.version}` })}
            </Dialog.Title>
            {released && (
              <div className="mt-0.5 text-xs text-faint">
                {t("update.released", { date: t.d(released, { dateStyle: "medium" }) })}
              </div>
            )}
            <div className="mt-4 min-h-0 flex-1 overflow-y-auto rounded-md bg-surface-2/40 p-3 text-sm">
              <ReleaseNotes markdown={update.notes} />
            </div>
            <a
              href={releaseUrl(update.version)}
              target="_blank"
              rel="noreferrer"
              className="mt-3 flex items-center gap-1 self-start text-xs text-dim hover:text-text hover:underline"
            >
              {t("update.viewRelease")} <ExternalLink className="size-3" />
            </a>
            <p className="mt-3 text-xs text-faint">{t("update.restartHint")}</p>
            {state.status === "error" && (
              <p className="mt-2 text-xs text-danger">
                {t("update.installFailed", { error: state.message })}
              </p>
            )}
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="outline" onClick={() => setOpen(false)} disabled={installing}>
                {t("update.later")}
              </Button>
              <Button onClick={() => void updateController.restartToUpdate()} disabled={installing}>
                <RotateCw className={`size-4 ${installing ? "animate-spin" : ""}`} />
                {installing ? t("update.installing") : t("update.restart")}
              </Button>
            </div>
          </Dialog.Content>
        )}
      </Dialog.Portal>
    </Dialog.Root>
  );
}
