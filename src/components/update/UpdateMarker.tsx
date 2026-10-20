import { CircleArrowUp } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useUpdateState, versionLabel } from "../../lib/updater";
import { useUiStore } from "../../stores/uiStore";

/**
 * "Update to vX.Y.Z", shown only while a downloaded update waits. It opens the
 * update panel; nothing installs until the listener says so there.
 */
export function UpdateMarker() {
  const t = useT();
  const state = useUpdateState();
  const openPanel = useUiStore((s) => s.setUpdatePanelOpen);
  if (state.status !== "ready") return null;

  return (
    <button
      onClick={() => openPanel(true)}
      className="glass-hover flex w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-accent transition-colors hover:bg-surface-2/60"
    >
      <CircleArrowUp className="size-4 shrink-0" />
      <span className="truncate">
        {t("update.marker", { version: versionLabel(state.update.version) })}
      </span>
    </button>
  );
}
