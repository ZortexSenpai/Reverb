// Album- and artist-level actions. Both entities only carry metadata in the
// lists they appear in, so every action first resolves the entity to its
// tracks (one request for an album, one per album for an artist).

import { ContextMenu, DropdownMenu } from "radix-ui";
import { Disc3, Heart, ListEnd, ListMusic, ListPlus, Play, Plus, Shuffle, User } from "lucide-react";
import type { ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import type { Album, Artist, Song } from "../../lib/subsonic/types";
import { linkableArtists } from "../../lib/artistCredits";
import { useT } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import { qk } from "../../lib/subsonic/keys";
import { usePlaylists, useStarMutation } from "../../lib/subsonic/queries";
import { usePlayerStore } from "../../stores/playerStore";
import { useUiStore } from "../../stores/uiStore";

const itemClass =
  "flex cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 text-sm text-text outline-none select-none data-highlighted:bg-surface-2";
export const menuContentClass =
  "z-50 min-w-52 rounded-lg border border-edge bg-surface p-1 shadow-xl";

type Kind = "album" | "artist";

interface Entity {
  kind: Kind;
  id: string;
  name: string;
  starred?: string;
}

function useEntityActions({ kind, id, name, starred }: Entity) {
  const t = useT();
  const setStar = useStarMutation();
  const queryClient = useQueryClient();
  const showToast = useUiStore((s) => s.showToast);
  const openNewPlaylist = useUiStore((s) => s.openNewPlaylist);
  const isStarred = Boolean(starred);

  async function songs(): Promise<Song[]> {
    return kind === "album"
      ? (await cmd("library_get_album", { id })).song
      : await cmd("library_get_artist_songs", { id });
  }

  /** Resolve the tracks, then hand them to `use`; toasts on failure. */
  function withSongs(use: (songs: Song[]) => void) {
    void (async () => {
      try {
        const list = await songs();
        if (list.length === 0) {
          showToast(t("toast.nothingToPlay"));
          return;
        }
        use(list);
      } catch (e) {
        showToast(errorMessage(e), "error");
      }
    })();
  }

  const { playContext, playNext, addToQueue } = usePlayerStore.getState();

  return {
    isStarred,
    play: () => withSongs((list) => playContext(list, 0)),
    shuffle: () => withSongs((list) => playContext(list, 0, { shuffle: true })),
    playNext: () =>
      withSongs((list) => {
        playNext(list);
        showToast(t("toast.playNext", { n: list.length }));
      }),
    addToQueue: () =>
      withSongs((list) => {
        addToQueue(list);
        showToast(t("toast.addedToQueue", { n: list.length }));
      }),
    newPlaylist: () => withSongs((list) => openNewPlaylist(list.map((s) => s.id))),
    addToPlaylist: (playlistId: string, playlistName: string) =>
      withSongs((list) => {
        void (async () => {
          try {
            await cmd("library_update_playlist", {
              playlistId,
              addSongIds: list.map((s) => s.id),
            });
            void queryClient.invalidateQueries({ queryKey: qk.playlist(playlistId) });
            void queryClient.invalidateQueries({ queryKey: qk.playlists() });
            showToast(
              t("toast.addedToPlaylist", { n: list.length, playlist: playlistName }),
            );
          } catch (e) {
            showToast(errorMessage(e), "error");
          }
        })();
      }),
    toggleStar: () => void setStar(id, kind, !isStarred),
    label: name,
  };
}

/** The action list, shared by the card context menus and the detail headers. */
function EntityItems({
  Menu,
  entity,
  navigateItems,
}: {
  Menu: typeof ContextMenu | typeof DropdownMenu;
  entity: Entity;
  navigateItems?: ReactNode;
}) {
  const t = useT();
  const actions = useEntityActions(entity);
  const { data: playlists } = usePlaylists();

  return (
    <>
      <Menu.Item className={itemClass} onSelect={actions.play}>
        <Play className="size-4 text-dim" /> {t("menu.playNow")}
      </Menu.Item>
      <Menu.Item className={itemClass} onSelect={actions.shuffle}>
        <Shuffle className="size-4 text-dim" /> {t("common.shuffle")}
      </Menu.Item>
      <Menu.Item className={itemClass} onSelect={actions.playNext}>
        <ListEnd className="size-4 text-dim" /> {t("menu.playNext")}
      </Menu.Item>
      <Menu.Item className={itemClass} onSelect={actions.addToQueue}>
        <ListPlus className="size-4 text-dim" /> {t("menu.addToQueue")}
      </Menu.Item>
      <Menu.Separator className="my-1 h-px bg-edge" />
      <Menu.Sub>
        <Menu.SubTrigger className={itemClass}>
          <ListMusic className="size-4 text-dim" /> {t("menu.addToPlaylist")}
        </Menu.SubTrigger>
        <Menu.Portal>
          <Menu.SubContent className="z-50 max-h-80 min-w-48 overflow-y-auto rounded-lg border border-edge bg-surface p-1 shadow-xl">
            <Menu.Item className={itemClass} onSelect={actions.newPlaylist}>
              <Plus className="size-4 text-dim" /> {t("menu.newPlaylist")}
            </Menu.Item>
            {(playlists ?? []).length > 0 && <Menu.Separator className="my-1 h-px bg-edge" />}
            {(playlists ?? []).map((playlist) => (
              <Menu.Item
                key={playlist.id}
                className={itemClass}
                onSelect={() => actions.addToPlaylist(playlist.id, playlist.name)}
              >
                <span className="truncate">{playlist.name}</span>
              </Menu.Item>
            ))}
          </Menu.SubContent>
        </Menu.Portal>
      </Menu.Sub>
      {navigateItems}
      <Menu.Separator className="my-1 h-px bg-edge" />
      <Menu.Item className={itemClass} onSelect={actions.toggleStar}>
        <Heart className={`size-4 ${actions.isStarred ? "fill-accent text-accent" : "text-dim"}`} />
        {actions.isStarred ? t("favourite.remove") : t("favourite.add")}
      </Menu.Item>
    </>
  );
}

export function AlbumContextMenu({ album, children }: { album: Album; children: ReactNode }) {
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className={menuContentClass}>
          <AlbumMenuItems album={album} />
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

/** The menu's items, inside a `ContextMenu.Content`; they mount only while it's open. */
export function AlbumMenuItems({ album }: { album: Album }) {
  const t = useT();
  const navigate = useNavigate();
  const artists = linkableArtists(album);
  return (
    <EntityItems
      Menu={ContextMenu}
      entity={{ kind: "album", id: album.id, name: album.name, starred: album.starred }}
      navigateItems={
        <>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Item
            className={itemClass}
            onSelect={() => navigate(`/albums/${album.id}`)}
          >
            <Disc3 className="size-4 text-dim" /> {t("menu.goToAlbum")}
          </ContextMenu.Item>
          {artists.length === 1 && (
            <ContextMenu.Item
              className={itemClass}
              onSelect={() => navigate(`/artists/${artists[0].id}`)}
            >
              <User className="size-4 text-dim" /> {t("menu.goToArtist")}
            </ContextMenu.Item>
          )}
          {/* Several credited album artists: pick which one to open. */}
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
      }
    />
  );
}

export function ArtistContextMenu({ artist, children }: { artist: Artist; children: ReactNode }) {
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className={menuContentClass}>
          <ArtistMenuItems artist={artist} />
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

/** The menu's items, inside a `ContextMenu.Content`; they mount only while it's open. */
export function ArtistMenuItems({ artist }: { artist: Artist }) {
  const t = useT();
  const navigate = useNavigate();
  return (
    <EntityItems
      Menu={ContextMenu}
      entity={{ kind: "artist", id: artist.id, name: artist.name, starred: artist.starred }}
      navigateItems={
        <>
          <ContextMenu.Separator className="my-1 h-px bg-edge" />
          <ContextMenu.Item
            className={itemClass}
            onSelect={() => navigate(`/artists/${artist.id}`)}
          >
            <User className="size-4 text-dim" /> {t("menu.goToArtist")}
          </ContextMenu.Item>
        </>
      }
    />
  );
}

/** Same actions as a "…" dropdown, for the album and artist detail headers. */
export function EntityActionsMenu({ entity, trigger }: { entity: Entity; trigger: ReactNode }) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>{trigger}</DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content align="start" className={menuContentClass}>
          <EntityItems Menu={DropdownMenu} entity={entity} />
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
