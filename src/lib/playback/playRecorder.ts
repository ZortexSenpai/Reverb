// Records a Play in the Play History once the engine counts one. The engine
// only knows the track id; the full Song (cover, album and artist ids) the
// history rows need comes from the Queue's current item.

import { cmd } from "../ipc/commands";
import type { EventMap } from "../ipc/events";
import { queryClient } from "../queryClient";
import { qk } from "../subsonic/keys";
import { usePlayerStore } from "../../stores/playerStore";

export async function recordPlay({ trackId, startedAtMs }: EventMap["player:play-counted"]) {
  const current = usePlayerStore.getState().currentItem();
  // The Queue moved on (or was cleared) before the event arrived; recording
  // whatever is current now would put the wrong Song in the history.
  if (current?.song.id !== trackId) {
    console.debug(`play of ${trackId} counted, but it is no longer current; not recorded`);
    return;
  }
  await cmd("history_record", { song: current.song, playedAt: startedAtMs });
  await queryClient.invalidateQueries({ queryKey: qk.playHistory() });
}
