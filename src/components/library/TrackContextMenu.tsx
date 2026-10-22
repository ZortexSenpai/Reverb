import { ContextMenu } from "radix-ui";
import {
  Disc3,
  Heart,
  ListEnd,
  ListMusic,
  ListPlus,
  ListX,
  Play,
  Plus,
  Radio,
  Star,
  User,
} from "lucide-react";
import type { ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import type { Song } from "../../lib/subsonic/types";
import { linkableArtists } from "../../lib/artistCredits";
import { useT } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { usePlaylists, useRatingMutation, useStarMutation } from "../../lib/subsonic/queries";
import { qk } from "../../lib/subsonic/keys";
import { usePlayerStore } from "../../stores/playerStore";
import { useUiStore } from "../../stores/uiStore";

interface Props {
  song: Song;
  /** The list the song lives in (used as play context). */
  context: Song[];
  index: number;
  /**
   * Rows the menu acts on. Defaults to the clicked song; a multi-selection
   * passes the whole selection so one menu covers all of it.
   */
  targets?: Song[];
  /** When set, offers "Remove from playlist" (used in playlist detail). */
  onRemoveFromPlaylist?: (songs: Song[]) => void;
  /**
   * When set, "Play now" hands it the clicked song instead of playing
   * `context` (for the Play History, which is no list to play through).
   */
  onPlayNow?: (song: Song) => void;
  children: ReactNode;
}

const itemClass =
  "flex cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 text-sm text-text outline-none select-none data-highlighted:bg-surface-2";

export function TrackContextMenu({
  song,
  context,
  index,
  targets,
  onRemoveFromPlaylist,
  onPlayNow,
  children,
}: Props) {
  const t = useT();
  const navigate = useNavigate();
  const setStar = useStarMutation();
  const setRating = useRatingMutation();
  const queryClient = useQueryClient();
  const { data: playlists } = usePlaylists();
  const showToast = useUiStore((s) => s.showToast);
  const openNewPlaylist = useUiStore((s) => s.openNewPlaylist);
  const { playContext, playNext, addToQueue, startRadio } = usePlayerStore.getState();

  const songs = targets && targets.length > 0 ? targets : [song];
  const many = songs.length > 1;
  const artists = linkableArtists(song);
  const suffix = many ? ` (${t.n(songs.length)})` : "";
  // A mixed selection stars everything; only an all-starred one unstars.
  const allStarred = songs.every((s) => Boolean(s.starred));
  const rating = many ? 0 : (song.userRating ?? 0);

  async function addToPlaylist(playlistId: string, playlistName: string) {
    try {
      await cmd("library_update_playlist", {
        playlistId,
        addSongIds: songs.map((s) => s.id),
      });
      void queryClient.invalidateQueries({ queryKey: qk.playlist(playlistId) });
      void queryClient.invalidateQueries({ queryKey: qk.playlists() });
      showToast(t("toast.addedToPlaylist", { n: songs.length, playlist: playlistName }));
    } catch (e) {
      showToast(errorMessage(e), "error");
    }
  }

  async function radio() {
    const added = await startRadio(song);
    if (added === 0) showToast(t("toast.radioEmpty", { title: song.title }), "error");
  }

  async function starAll() {
    for (const s of songs) {
      if (Boolean(s.starred) === allStarred) await setStar(s.id, "song", !allStarred);
    }
  }

  async function rateAll(value: number) {
    for (const s of songs) await setRating(s.id, value);
  }

  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="z-50 min-w-52 rounded-lg border border-edge bg-surface p-1 shadow-xl">
          <ContextMenu.Item
            className={itemClass}
            onSelect={() => {
              if (onPlayNow) onPlayNow(song);
              else if (many) playContext(songs, 0);
              else playContext(context, index);
            }}
          >
            <Play className="size-4 text-dim" /> {t("menu.playNow")}
            {suffix}
          </ContextMenu.Item>
          <ContextMenu.Item className={itemClass} onSelect={() => playNext(songs)}>
            <ListEnd className="size-4 text-dim" /> {t("menu.playNext")}
            {suffix}
          </ContextMenu.Item>
          <ContextMenu.Item className={itemClass} onSelect={() => addToQueue(songs)}>
            <ListPlus className="size-4 text-dim" /> {t("menu.addToQueue")}
            {suffix}
          </ContextMenu.Item>
          {!many && (
            <ContextMenu.Item className={itemClass} onSelect={() => void radio()}>
              <Radio className="size-4 text-dim" /> {t("menu.startRadio")}
            </ContextMenu.Item>
          )}
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger className={itemClass}>
              <ListMusic className="size-4 text-dim" /> {t("menu.addToPlaylist")}
            </ContextMenu.SubTrigger>
            <ContextMenu.Portal>
              <ContextMenu.SubContent className="z-50 max-h-80 min-w-48 overflow-y-auto rounded-lg border border-edge bg-surface p-1 shadow-xl">
                <ContextMenu.Item
                  className={itemClass}
                  onSelect={() => openNewPlaylist(songs.map((s) => s.id))}
                >
                  <Plus className="size-4 text-dim" /> {t("menu.newPlaylist")}
                </ContextMenu.Item>
                {(playlists ?? []).length > 0 && (
                  <ContextMenu.Separator className="my-1 h-px bg-edge" />
                )}
                {(playlists ?? []).map((playlist) => (
                  <ContextMenu.Item
                    key={playlist.id}
                    className={itemClass}
                    onSelect={() => void addToPlaylist(playlist.id, playlist.name)}
                  >
                    <span className="truncate">{playlist.name}</span>
                  </ContextMenu.Item>
                ))}
              </ContextMenu.SubContent>
            </ContextMenu.Portal>
          </ContextMenu.Sub>
          {onRemoveFromPlaylist && (
            <ContextMenu.Item
              className={itemClass}
              onSelect={() => onRemoveFromPlaylist(songs)}
            >
              <ListX className="size-4 text-dim" /> {t("menu.removeFromPlaylist")}
              {suffix}
            </ContextMenu.Item>
          )}
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger className={itemClass}>
              <Star className="size-4 text-dim" /> {t("menu.rate")}
              {suffix}
            </ContextMenu.SubTrigger>
            <ContextMenu.Portal>
              <ContextMenu.SubContent className="z-50 min-w-40 rounded-lg border border-edge bg-surface p-1 shadow-xl">
                {[5, 4, 3, 2, 1].map((value) => (
                  <ContextMenu.Item
                    key={value}
                    className={itemClass}
                    onSelect={() => void rateAll(value)}
                  >
                    <span className="flex items-center">
                      {[1, 2, 3, 4, 5].map((star) => (
                        <Star
                          key={star}
                          className={`size-3.5 ${
                            star <= value ? "fill-accent text-accent" : "text-faint"
                          }`}
                        />
                      ))}
                    </span>
                    {rating === value && (
                      <span className="ml-1 text-xs text-faint">{t("menu.currentRating")}</span>
                    )}
                  </ContextMenu.Item>
                ))}
                <ContextMenu.Separator className="my-1 h-px bg-edge" />
                <ContextMenu.Item className={itemClass} onSelect={() => void rateAll(0)}>
                  {t("rating.clear")}
                </ContextMenu.Item>
              </ContextMenu.SubContent>
            </ContextMenu.Portal>
          </ContextMenu.Sub>
          <ContextMenu.Item className={itemClass} onSelect={() => void starAll()}>
            <Heart className={`size-4 ${allStarred ? "fill-accent text-accent" : "text-dim"}`} />
            {allStarred ? t("favourite.remove") : t("favourite.add")}
            {suffix}
          </ContextMenu.Item>
          {!many && (song.albumId || artists.length > 0) && (
            <>
              <ContextMenu.Separator className="my-1 h-px bg-edge" />
              {song.albumId && (
                <ContextMenu.Item
                  className={itemClass}
                  onSelect={() => navigate(`/albums/${song.albumId}`)}
                >
                  <Disc3 className="size-4 text-dim" /> {t("menu.goToAlbum")}
                </ContextMenu.Item>
              )}
              {artists.length === 1 && (
                <ContextMenu.Item
                  className={itemClass}
                  onSelect={() => navigate(`/artists/${artists[0].id}`)}
                >
                  <User className="size-4 text-dim" /> {t("menu.goToArtist")}
                </ContextMenu.Item>
              )}
              {/* Several credited artists: pick which one to open. */}
              {artists.length > 1 && (
                <ContextMenu.Sub>
                  <ContextMenu.SubTrigger className={itemClass}>
                    <User className="size-4 text-dim" /> {t("menu.goToArtist")}
                  </ContextMenu.SubTrigger>
                  <ContextMenu.Portal>
                    <ContextMenu.SubContent className="z-50 min-w-40 rounded-lg border border-edge bg-surface p-1 shadow-xl">
                      {artists.map((artist) => (
                        <ContextMenu.Item
                          key={artist.id}
                          className={itemClass}
                          onSelect={() => navigate(`/artists/${artist.id}`)}
                        >
                          <span className="truncate">{artist.name}</span>
                        </ContextMenu.Item>
                      ))}
                    </ContextMenu.SubContent>
                  </ContextMenu.Portal>
                </ContextMenu.Sub>
              )}
            </>
          )}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}
