import type { RefObject } from "react";
import type { Album } from "../../lib/subsonic/types";
import { GRID_CARD_WIDTH, usePrefsStore } from "../../stores/prefsStore";
import { AlbumCard } from "./AlbumCard";
import { CardGrid } from "./CardGrid";

interface Props {
  albums: Album[];
  scrollRef: RefObject<HTMLDivElement | null>;
  onEndReached?: () => void;
}

export function AlbumGrid({ albums, scrollRef, onEndReached }: Props) {
  const gridSize = usePrefsStore((s) => s.prefs.gridSize);
  return (
    <CardGrid
      items={albums}
      itemKey={(album) => album.id}
      renderItem={(album) => <AlbumCard album={album} />}
      scrollRef={scrollRef}
      onEndReached={onEndReached}
      cardMinWidth={GRID_CARD_WIDTH[gridSize] ?? 190}
    />
  );
}
