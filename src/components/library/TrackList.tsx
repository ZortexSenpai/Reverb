import type { Song } from "../../lib/subsonic/types";
import { useSelection } from "../../lib/useSelection";
import { usePlayerStore } from "../../stores/playerStore";
import { SelectionBar } from "./SelectionBar";
import { TrackContextMenu } from "./TrackContextMenu";
import { TrackRow } from "./TrackRow";

interface Props {
  songs: Song[];
  showArt?: boolean;
  showArtist?: boolean;
}

/** Plain track list — playing a row sets the whole list as the play context. */
export function TrackList({ songs, showArt, showArtist }: Props) {
  const playContext = usePlayerStore((s) => s.playContext);
  const selection = useSelection(songs);
  return (
    <div className="flex flex-col">
      <SelectionBar songs={selection.selected} onClear={selection.clear} />
      {songs.map((song, i) => (
        <TrackContextMenu
          key={`${song.id}-${i}`}
          song={song}
          context={songs}
          index={i}
          targets={selection.targetsFor(song)}
        >
          <div onContextMenu={() => selection.onRowContextMenu(i)}>
            <TrackRow
              song={song}
              index={i}
              showArt={showArt}
              showArtist={showArtist}
              selected={selection.isSelected(song.id)}
              onPlay={() => playContext(songs, i)}
              onClick={(e) => selection.onRowClick(i, e)}
            />
          </div>
        </TrackContextMenu>
      ))}
    </div>
  );
}
