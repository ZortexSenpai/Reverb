import { ListMusic, Maximize2, SlidersVertical } from "lucide-react";
import { useT } from "../../lib/i18n";
import { playerBarAccent } from "../../lib/themes/genshin";
import { usePlayerStore } from "../../stores/playerStore";
import { usePrefsStore } from "../../stores/prefsStore";
import { useUiStore } from "../../stores/uiStore";
import { ArtistLinks } from "../library/ArtistLinks";
import { CoverImage } from "../library/CoverImage";
import { StarButton } from "../library/StarButton";
import { IconButton } from "../ui/Button";
import { SeekBar } from "../player/SeekBar";
import { SignalPathIndicator } from "../player/SignalPathIndicator";
import { TransportControls } from "../player/TransportControls";
import { VolumeSlider } from "../player/VolumeSlider";
import { useNavigate } from "react-router";

/**
 * Theme decoration perched on the bar's top edge (Genshin themes): an ornament
 * trim along the edge plus a small accent image peeking over it. Purely
 * decorative, so it never takes pointer events.
 */
function PlayerBarAccent() {
  const accent = playerBarAccent(usePrefsStore((s) => s.prefs.theme));
  if (!accent) return null;
  return (
    <>
      <span aria-hidden className="player-bar-trim" />
      <span
        aria-hidden
        className={`player-bar-accent ${accent.character ? "player-bar-accent-character" : ""}`}
        style={{ backgroundImage: `url("${accent.url}")` }}
      />
    </>
  );
}

export function PlayerBar() {
  const t = useT();
  const item = usePlayerStore((s) => s.queue[s.queueIndex] ?? null);
  const { setQueuePanelOpen, setNowPlayingOpen } = useUiStore.getState();
  const queuePanelOpen = useUiStore((s) => s.queuePanelOpen);
  const liquid = usePrefsStore((s) => s.prefs.layout) === "liquid";
  const navigate = useNavigate();

  const song = item?.song;

  return (
    <footer
      className={`relative flex h-[84px] shrink-0 items-center gap-4 px-4 ${
        liquid ? "glass rounded-2xl" : "classic-panel border-t border-edge"
      }`}
    >
      <PlayerBarAccent />
      {/* Track info */}
      <div className="flex w-64 min-w-0 items-center gap-3">
        {song ? (
          <>
            <button
              className="cursor-pointer transition-transform hover:scale-105"
              onClick={() => setNowPlayingOpen(true)}
              aria-label={t("player.openFullScreen")}
            >
              <CoverImage coverArt={song.coverArt} size={96} alt="" className="size-12 rounded" />
            </button>
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium">{song.title}</div>
              <div className="truncate text-xs text-dim">
                <ArtistLinks credit={song} />
              </div>
            </div>
            {/* Sits with the track it applies to, as in the full-screen player.
                The title truncates to make room rather than pushing it out. */}
            <StarButton
              id={song.id}
              kind="song"
              starred={song.starred}
              className="shrink-0"
            />
          </>
        ) : (
          <div className="text-xs text-faint">{t("player.nothingPlaying")}</div>
        )}
      </div>

      {/* Transport + seek */}
      <div className="flex min-w-0 flex-1 flex-col items-center gap-1">
        <TransportControls />
        <div className="w-full max-w-xl">
          <SeekBar />
        </div>
      </div>

      {/* Right controls */}
      <div className="flex w-64 items-center justify-end gap-1">
        <SignalPathIndicator />
        <IconButton label={t("player.eqInSettings")} onClick={() => navigate("/settings")}>
          <SlidersVertical className="size-4" />
        </IconButton>
        <IconButton
          label={t("queue.title")}
          onClick={() => setQueuePanelOpen(!queuePanelOpen)}
          className={queuePanelOpen ? "text-accent" : ""}
        >
          <ListMusic className="size-4" />
        </IconButton>
        <IconButton
          label={t("player.fullScreen")}
          onClick={() => setNowPlayingOpen(true)}
          disabled={!song}
        >
          <Maximize2 className="size-4" />
        </IconButton>
        <VolumeSlider />
      </div>
    </footer>
  );
}
