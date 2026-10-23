// The Period's top artists over time, as small multiples: one row per artist,
// its entry beside an accent area chart on the activity timeline's buckets.
// Every row shares one scale, so they compare at a glance without a colour
// per artist.

import { AreaChart } from "./charts";
import { StatsEntry, statsEntryKey } from "./StatsEntry";
import type { ArtistTimeline } from "../../lib/subsonic/types";

export function ArtistsOverTime({ artists }: { artists: ArtistTimeline[] }) {
  const max = Math.max(1, ...artists.flatMap((a) => a.timeline.map((point) => point.count)));
  return (
    <ol className="flex flex-col">
      {artists.map(({ artist, timeline }, i) => (
        <li key={statsEntryKey(artist)} className="grid grid-cols-[16rem_minmax(0,1fr)] gap-4">
          <StatsEntry kind="artist" item={artist} layout="row" count={artist.count} />
          {/* Level with the avatar; the last row carries the bucket labels. */}
          <div className="pt-1.5">
            <AreaChart
              points={timeline}
              max={max}
              className="h-10"
              maxLabels={i === artists.length - 1 ? 6 : 0}
            />
          </div>
        </li>
      ))}
    </ol>
  );
}
