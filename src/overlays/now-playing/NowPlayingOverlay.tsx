import { AnimatePresence, motion } from "motion/react";
import {
  ChevronDown,
  ListMusic,
  MicVocal,
  SlidersVertical,
} from "lucide-react";
import { Fragment, useState } from "react";
import { Link, useNavigate } from "react-router";
import { ArtistLinks } from "../../components/library/ArtistLinks";
import { CoverImage } from "../../components/library/CoverImage";
import { EntityGenreBubbles } from "../../components/library/GenreBubbles";
import { StarButton } from "../../components/library/StarButton";
import { IconButton } from "../../components/ui/Button";
import { SeekBar } from "../../components/player/SeekBar";
import { SignalPathIndicator } from "../../components/player/SignalPathIndicator";
import { TransportControls } from "../../components/player/TransportControls";
import { VolumeSlider } from "../../components/player/VolumeSlider";
import { artUrl } from "../../lib/art";
import { useT } from "../../lib/i18n";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";
import type { NowPlayingArtSize, NowPlayingBackdrop } from "../../lib/subsonic/types";
import { LyricsPanel } from "./LyricsPanel";
import { activeFieldValues } from "./nowPlayingFields";

// Static class strings so Tailwind sees them; sharper styles request larger
// art. "solid" renders no backdrop at all (the plain bg-bg shows through).
const BACKDROPS: Record<NowPlayingBackdrop, { className: string; artSize: number } | null> = {
  blurred: { className: "scale-150 blur-[90px] brightness-[0.35] saturate-150", artSize: 300 },
  soft: { className: "scale-125 blur-[32px] brightness-[0.4] saturate-[1.25]", artSize: 600 },
  clear: { className: "scale-105 brightness-[0.45]", artSize: 1200 },
  solid: null,
};

// Cover width, capped in both vh and px so the art neither overflows a short
// window nor balloons on a large one. The `lyrics` variant is the same scale
// shrunk to leave room for the lyrics panel beside it.
const ART_SIZES: Record<NowPlayingArtSize, { alone: string; lyrics: string }> = {
  small: { alone: "w-[min(32vh,340px)]", lyrics: "w-[min(24vh,240px)]" },
  medium: { alone: "w-[min(44vh,480px)]", lyrics: "w-[min(32vh,340px)]" },
  large: { alone: "w-[min(56vh,620px)]", lyrics: "w-[min(40vh,440px)]" },
};

export function NowPlayingOverlay() {
  const t = useT();
  const open = useUiStore((s) => s.nowPlayingOpen);
  const setOpen = useUiStore((s) => s.setNowPlayingOpen);
  const setQueuePanelOpen = useUiStore((s) => s.setQueuePanelOpen);
  const queuePanelOpen = useUiStore((s) => s.queuePanelOpen);
  const item = usePlayerStore((s) => s.queue[s.queueIndex] ?? null);
  const backdropPref = usePrefsStore((s) => s.prefs.nowPlayingBackdrop);
  const artSizePref = usePrefsStore((s) => s.prefs.nowPlayingArtSize);
  const alignPref = usePrefsStore((s) => s.prefs.nowPlayingAlign);
  const fieldPrefs = usePrefsStore((s) => s.prefs.nowPlayingFields);
  const genreBubbles = usePrefsStore((s) => s.prefs.nowPlayingGenres);
  const navigate = useNavigate();
  const [lyricsOpen, setLyricsOpen] = useState(false);

  const song = item?.song;
  // Key check, not `??`: "solid" is stored as null and must stay null.
  const backdrop = backdropPref in BACKDROPS ? BACKDROPS[backdropPref] : BACKDROPS.blurred;
  const artSize = ART_SIZES[artSizePref] ?? ART_SIZES.medium;
  const alignLeft = alignPref === "left";
  const fields = song ? activeFieldValues(song, fieldPrefs, t) : [];

  return (
    <AnimatePresence>
      {open && song && (
        <motion.div
          initial={{ y: "100%" }}
          animate={{ y: 0 }}
          exit={{ y: "100%" }}
          transition={{ type: "tween", duration: 0.25, ease: "easeOut" }}
          className="fixed inset-0 z-20 flex flex-col overflow-hidden bg-bg"
        >
          {/* Ambient album-art backdrop (style is a user preference) */}
          {backdrop && (
            <>
              <div
                className={`absolute inset-0 bg-cover bg-center ${backdrop.className}`}
                style={{ backgroundImage: `url(${artUrl(song.coverArt, backdrop.artSize)})` }}
              />
              <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-bg/70" />
            </>
          )}

          {/* Top bar */}
          <div className="relative z-10 flex items-center justify-between px-6 pt-5">
            <IconButton
              label={t("player.closeFullScreen")}
              onClick={() => setOpen(false)}
            >
              <ChevronDown className="size-5" />
            </IconButton>
            <div className="flex items-center gap-1">
              <IconButton
                label={t("player.lyrics")}
                onClick={() => setLyricsOpen(!lyricsOpen)}
                className={lyricsOpen ? "text-accent" : ""}
              >
                <MicVocal className="size-4" />
              </IconButton>
              <IconButton
                label={t("player.eqInSettings")}
                onClick={() => {
                  setOpen(false);
                  navigate("/settings");
                }}
              >
                <SlidersVertical className="size-4" />
              </IconButton>
              <IconButton
                label={t("queue.title")}
                onClick={() => setQueuePanelOpen(!queuePanelOpen)}
                className={queuePanelOpen ? "text-accent" : ""}
              >
                <ListMusic className="size-4" />
              </IconButton>
            </div>
          </div>

          {/* Center: art + meta (+ lyrics) */}
          <div
            className={`relative z-10 flex min-h-0 flex-1 items-center justify-center px-8 ${
              lyricsOpen ? "gap-14" : ""
            }`}
          >
            {/* The column is sized by its widest child, so short metadata sits
                exactly as wide as the cover and only a long title stretches it
                (to max-w-2xl, then truncating). That keeps left-ranged text
                lined up with the cover's edge without pinning a width. */}
            <div
              className={`flex min-w-0 flex-col justify-center gap-7 ${
                alignLeft ? "items-start" : "items-center"
              }`}
            >
              <CoverImage
                coverArt={song.coverArt}
                size={1200}
                alt={song.album ?? song.title}
                className={`aspect-square rounded-xl shadow-2xl ${
                  lyricsOpen ? artSize.lyrics : artSize.alone
                }`}
              />
              <div
                className={`flex w-full max-w-2xl flex-col ${
                  alignLeft ? "items-start text-left" : "items-center text-center"
                }`}
              >
                <div
                  className={`flex w-full items-center gap-2 ${
                    alignLeft ? "" : "justify-center"
                  }`}
                >
                  <h1 className="min-w-0 truncate text-3xl font-bold">{song.title}</h1>
                  <StarButton id={song.id} kind="song" starred={song.starred} />
                </div>
                <div className="mt-1.5 w-full truncate text-base text-dim">
                  <ArtistLinks credit={song} onNavigate={() => setOpen(false)} />
                </div>
                {song.album && (
                  <div className="mt-1 w-full truncate text-sm text-dim">
                    {song.albumId ? (
                      <Link
                        to={`/albums/${song.albumId}`}
                        onClick={() => setOpen(false)}
                        className="hover:text-text hover:underline"
                      >
                        {song.album}
                      </Link>
                    ) : (
                      <span>{song.album}</span>
                    )}
                  </div>
                )}
                {genreBubbles && (
                  <EntityGenreBubbles
                    of={song}
                    onNavigate={() => setOpen(false)}
                    className={`mt-3 ${alignLeft ? "" : "justify-center"}`}
                  />
                )}
                {fields.length > 0 && (
                  <div
                    className={`mt-3.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-faint ${
                      alignLeft ? "" : "justify-center"
                    }`}
                  >
                    {fields.map((field, i) => (
                      <Fragment key={field.id}>
                        {i > 0 && (
                          <span aria-hidden className="opacity-50">
                            ·
                          </span>
                        )}
                        <span>{field.value}</span>
                      </Fragment>
                    ))}
                  </div>
                )}
              </div>
            </div>
            {lyricsOpen && (
              <div className="h-[68vh] w-[min(42vw,560px)] shrink-0">
                <LyricsPanel key={song.id} song={song} />
              </div>
            )}
          </div>

          {/* Bottom: controls */}
          <div className="relative z-10 flex flex-col items-center gap-2 px-8 pb-8">
            <div className="w-full max-w-2xl">
              <SeekBar />
            </div>
            <div className="relative flex w-full max-w-2xl items-center justify-center">
              <TransportControls />
              <div className="absolute right-0 flex items-center gap-1">
                <SignalPathIndicator />
                <VolumeSlider />
              </div>
            </div>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
