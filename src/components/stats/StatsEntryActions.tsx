// Makes a stats entry behave like its counterpart in the library views: a
// matched artist or album opens its page on click, a matched Song plays on
// double-click, and right-clicking opens the library's own context menu. The
// stats page only knows a Library match's id, so the menu fetches the item
// when it opens. An entry without a match is left as it is.

import { ContextMenu, Slot } from "radix-ui";
import type { ReactElement } from "react";
import { useQueryClient, type UseQueryResult } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { AlbumMenuItems, ArtistMenuItems, menuContentClass } from "../library/EntityContextMenu";
import { SongMenuItems } from "../library/TrackContextMenu";
import { useT } from "../../lib/i18n";
import { errorMessage } from "../../lib/ipc/commands";
import {
  statsEntryAction,
  type StatsEntryAction,
  type StatsEntryKind,
} from "../../lib/statsEntryAction";
import { songQuery, useAlbum, useArtist, useSong } from "../../lib/subsonic/queries";
import type { StatsItem } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";
import { useUiStore } from "../../stores/uiStore";

/**
 * Wraps one element (a DOM element, or a component forwarding its ref) that
 * shows `item`. Anything on the stats page that shows an artist, album or
 * Song goes through here, so they all behave alike.
 */
export function StatsEntryActions({
  kind,
  item,
  className = "",
  children,
}: {
  kind: StatsEntryKind;
  item: StatsItem;
  /** Added to the element only for a Library match, e.g. a hover highlight. */
  className?: string;
  children: ReactElement;
}) {
  const navigate = useNavigate();
  const playSong = usePlaySong();
  const action = statsEntryAction(kind, item);
  if (!action) return children;

  // A Song has no page; double-clicking plays it, as Song lists do (and, as
  // there, doesn't select the word under the pointer).
  const interaction =
    action.kind === "song"
      ? { onDoubleClick: () => playSong(action.id), className: "select-none" }
      : { onClick: () => navigate(action.page), className: "" };
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>
        <Slot.Root
          {...interaction}
          className={`cursor-pointer ${interaction.className} ${className}`}
        >
          {children}
        </Slot.Root>
      </ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className={menuContentClass}>
          <MenuItems action={action} />
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

/**
 * Plays the Song next and jumps to it, as the Play History does: a stats list
 * is no list to play through.
 */
function usePlaySong() {
  const queryClient = useQueryClient();
  const showToast = useUiStore((s) => s.showToast);
  return (id: string) => {
    void (async () => {
      try {
        const song = await queryClient.fetchQuery(songQuery(id));
        usePlayerStore.getState().playNow(song);
      } catch (e) {
        showToast(errorMessage(e), "error");
      }
    })();
  };
}

/** The library's menu for the item; only mounted while the menu is open. */
function MenuItems({ action }: { action: StatsEntryAction }) {
  switch (action.kind) {
    case "artist":
      return <ArtistItems id={action.id} />;
    case "album":
      return <AlbumItems id={action.id} />;
    case "song":
      return <SongItems id={action.id} />;
  }
}

function ArtistItems({ id }: { id: string }) {
  const artist = useArtist(id);
  return artist.data ? <ArtistMenuItems artist={artist.data} /> : <Pending query={artist} />;
}

function AlbumItems({ id }: { id: string }) {
  const album = useAlbum(id);
  return album.data ? <AlbumMenuItems album={album.data} /> : <Pending query={album} />;
}

function SongItems({ id }: { id: string }) {
  const song = useSong(id);
  if (!song.data) return <Pending query={song} />;
  const { playNow } = usePlayerStore.getState();
  return <SongMenuItems song={song.data} context={[song.data]} index={0} onPlayNow={playNow} />;
}

/** One line in place of the menu while the item loads, or if it can't. */
function Pending({ query }: { query: UseQueryResult }) {
  const t = useT();
  return (
    <ContextMenu.Label className="px-2.5 py-1.5 text-sm text-faint">
      {query.isError ? errorMessage(query.error) : t("common.loading")}
    </ContextMenu.Label>
  );
}
