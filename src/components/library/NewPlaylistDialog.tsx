// Global "new playlist" dialog, opened from the playlists view or the
// track context menu (optionally seeded with songs to add).

import { useQueryClient } from "@tanstack/react-query";
import { useT } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { qk } from "../../lib/subsonic/keys";
import { useUiStore } from "../../stores/uiStore";
import { TextPromptDialog } from "../ui/Dialogs";

export function NewPlaylistDialog() {
  const t = useT();
  const songIds = useUiStore((s) => s.newPlaylistSongIds);
  const initialName = useUiStore((s) => s.newPlaylistName);
  const close = useUiStore((s) => s.closeNewPlaylist);
  const showToast = useUiStore((s) => s.showToast);
  const queryClient = useQueryClient();

  async function create(name: string) {
    try {
      await cmd("library_create_playlist", { name, songIds: songIds ?? [] });
      void queryClient.invalidateQueries({ queryKey: qk.playlists() });
      showToast(
        songIds && songIds.length > 0
          ? t("toast.playlistCreatedWith", { name, n: songIds.length })
          : t("toast.playlistCreated", { name }),
      );
    } catch (e) {
      showToast(errorMessage(e), "error");
    }
  }

  return (
    <TextPromptDialog
      open={songIds !== null}
      title={t("playlists.new")}
      placeholder={t("playlists.namePlaceholder")}
      initialValue={initialName}
      confirmLabel={t("common.create")}
      onConfirm={(name) => void create(name)}
      onClose={close}
    />
  );
}
