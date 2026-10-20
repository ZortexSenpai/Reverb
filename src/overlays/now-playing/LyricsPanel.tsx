// Lyrics pane for the full-screen player. Synced lyrics highlight and
// auto-scroll with playback (driven by the position clock); clicking a
// line seeks. Falls back to plain lyrics, or a friendly empty state.

import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { MicVocal } from "lucide-react";
import { CenteredSpinner } from "../../components/ui/Spinner";
import { useT } from "../../lib/i18n";
import { cmd } from "../../lib/ipc/commands";
import { positionClock } from "../../lib/playback/positionClock";
import type { Song } from "../../lib/subsonic/types";
import { usePlayerStore } from "../../stores/playerStore";

export function LyricsPanel({ song }: { song: Song }) {
  const t = useT();
  const seekMs = usePlayerStore((s) => s.seekMs);
  const containerRef = useRef<HTMLDivElement>(null);
  const [activeIndex, setActiveIndex] = useState(-1);

  const query = useQuery({
    queryKey: ["lyrics", song.id],
    queryFn: () => cmd("library_get_lyrics", { id: song.id }),
    staleTime: Infinity,
  });

  const lyrics = useMemo(() => {
    const all = query.data?.structuredLyrics ?? [];
    return all.find((l) => l.synced && l.line.length > 0) ?? all.find((l) => l.line.length > 0) ?? null;
  }, [query.data]);

  // Track the active line against the playback clock.
  useEffect(() => {
    setActiveIndex(-1);
    if (!lyrics?.synced) return;
    const offset = lyrics.offset ?? 0;
    const timer = setInterval(() => {
      const pos = positionClock.now();
      let index = -1;
      for (let i = 0; i < lyrics.line.length; i++) {
        const start = lyrics.line[i].start;
        if (start == null) continue;
        if (start + offset <= pos) index = i;
        else break;
      }
      setActiveIndex((prev) => (prev === index ? prev : index));
    }, 250);
    return () => clearInterval(timer);
  }, [lyrics]);

  // Keep the active line centered.
  useEffect(() => {
    if (activeIndex < 0) return;
    const el = containerRef.current?.querySelector(`[data-line="${activeIndex}"]`);
    el?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [activeIndex]);

  if (query.isPending) return <CenteredSpinner />;

  if (!lyrics) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-faint">
        <MicVocal className="size-8" />
        <p className="text-sm">{t("lyrics.none")}</p>
      </div>
    );
  }

  const offset = lyrics.offset ?? 0;

  return (
    <div
      ref={containerRef}
      className="h-full overflow-y-auto px-2 py-[38%] [-ms-overflow-style:none] [scrollbar-width:none] [mask-image:linear-gradient(transparent,black_12%,black_88%,transparent)] [&::-webkit-scrollbar]:hidden"
    >
      {lyrics.line.map((line, i) => {
        const seekable = lyrics.synced && line.start != null;
        return (
          <button
            key={i}
            data-line={i}
            disabled={!seekable}
            onClick={() => seekable && seekMs((line.start ?? 0) + offset)}
            className={`block w-full rounded-md px-3 py-2 text-left text-xl leading-snug font-semibold transition-colors duration-300 ${
              i === activeIndex
                ? "text-text"
                : lyrics.synced
                  ? "text-faint hover:text-dim"
                  : "text-dim"
            } ${seekable ? "cursor-pointer" : ""}`}
          >
            {line.value || "•"}
          </button>
        );
      })}
    </div>
  );
}
